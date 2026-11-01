mod accel;
mod header;
mod kinds;
mod page;
mod paint;
mod regions;
mod resources;
mod shared_mem;

pub(crate) use shared_mem::{PaintLease, PaintSlots};

use crate::osr::transport::IpcStream;
use std::io::{self, Read};

#[cfg(target_os = "macos")]
use crate::osr::accel::SurfaceRegistry;
use crate::osr::protocol::OsrMessage;
#[cfg(target_os = "macos")]
use std::sync::Arc;

use accel::parse_accel_frame;
#[cfg(target_os = "linux")]
use accel::parse_dmabuf_announcement;
#[cfg(any(windows, target_os = "linux"))]
use accel::parse_retired_resources;
use header::{read_header, read_i32, read_u32};
use kinds::*;
use page::{
    parse_cursor, parse_drag_start, parse_guest_capture, parse_guest_order, parse_ime_surrounding,
    parse_json,
};
use paint::{BatchHeader, BatchSurface, parse_inline_batch};
use regions::parse_draggable_regions;
use resources::SharedResources;

pub(super) const HEADER_LEN: usize = 28;
pub(super) const MAGIC: &[u8; 4] = b"SAB1";
const MAX_SURFACE_DIMENSION: u32 = 16_384;
pub(super) const MAX_PAINT_BYTES: usize = 256 * 1024 * 1024;
const MAX_CONTROL_BYTES: usize = 64 * 1024 * 1024;
const MAX_HELLO_BYTES: usize = 64;
pub(super) const BATCH_ENTRY_LEN: usize = 28;

/// One message as it came off the wire, before it is understood.
struct RawMessage {
    kind: u32,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    payload: Vec<u8>,
    #[cfg(unix)]
    fd: header::ReceivedFd,
}

pub(crate) struct WireReader {
    stream: IpcStream,
    slots: PaintSlots,
    resources: SharedResources,
}

impl WireReader {
    pub(crate) fn new(
        stream: IpcStream,
        slots: PaintSlots,
        #[cfg(target_os = "macos")] surfaces: Arc<SurfaceRegistry>,
    ) -> Self {
        Self {
            stream,
            slots,
            #[cfg(target_os = "macos")]
            resources: SharedResources::new(surfaces),
            #[cfg(not(target_os = "macos"))]
            resources: SharedResources::default(),
        }
    }

    pub(crate) fn read_host_protocol(&mut self) -> io::Result<Option<String>> {
        let Some(header) = read_header(&mut self.stream)? else {
            return Ok(None);
        };
        #[cfg(unix)]
        let (header, _) = header;
        let payload_len = read_u32(&header[24..28]) as usize;
        if &header[0..4] != MAGIC
            || read_u32(&header[4..8]) != KIND_HOST_HELLO
            || payload_len > MAX_HELLO_BYTES
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the browser host did not identify its protocol",
            ));
        }
        let mut payload = vec![0_u8; payload_len];
        self.stream.read_exact(&mut payload)?;
        String::from_utf8(payload)
            .map(Some)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Reads the next message for the window, taking in the shared resources
    /// the browser host announces or retires between them.
    pub(crate) fn read(&mut self) -> io::Result<Option<OsrMessage>> {
        loop {
            let Some(raw) = self.read_raw()? else {
                return Ok(None);
            };
            if let Some(message) = self.parse(raw)? {
                return Ok(Some(message));
            }
        }
    }

    fn read_raw(&mut self) -> io::Result<Option<RawMessage>> {
        let Some(header) = read_header(&mut self.stream)? else {
            return Ok(None);
        };
        #[cfg(unix)]
        let (header, fd) = header;
        if &header[0..4] != MAGIC {
            return Err(invalid("invalid OSR message magic"));
        }
        let kind = read_u32(&header[4..8]);
        let width = read_u32(&header[8..12]);
        let height = read_u32(&header[12..16]);
        let payload_len = read_u32(&header[24..28]) as usize;
        if width > MAX_SURFACE_DIMENSION || height > MAX_SURFACE_DIMENSION {
            return Err(invalid("OSR surface dimensions exceed the protocol limit"));
        }
        let paint = is_paint_kind(kind);
        if paint
            && (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .is_none_or(|bytes| bytes > MAX_PAINT_BYTES)
        {
            return Err(invalid("OSR surface exceeds the protocol byte limit"));
        }
        let payload_limit = if paint {
            MAX_PAINT_BYTES
        } else {
            MAX_CONTROL_BYTES
        };
        if payload_len > payload_limit {
            return Err(invalid("OSR payload exceeds the protocol limit"));
        }
        let mut payload = vec![0_u8; payload_len];
        if payload_len > 0 {
            self.stream.read_exact(&mut payload)?;
        }
        Ok(Some(RawMessage {
            kind,
            width,
            height,
            x: read_i32(&header[16..20]),
            y: read_i32(&header[20..24]),
            payload,
            #[cfg(unix)]
            fd,
        }))
    }

    /// The message `raw` holds for the window, or none when it only updates
    /// the resources this reader keeps.
    fn parse(&mut self, raw: RawMessage) -> io::Result<Option<OsrMessage>> {
        let RawMessage {
            kind,
            width,
            height,
            x,
            y,
            payload,
            #[cfg(unix)]
            mut fd,
        } = raw;
        let batch_header = BatchHeader {
            width,
            height,
            x,
            y,
        };
        let message = match kind {
            KIND_MAIN_BATCH | KIND_POPUP_BATCH | KIND_GUEST_BATCH => OsrMessage::PaintBatch(
                parse_inline_batch(batch_surface(kind), batch_header, payload)?,
            ),
            KIND_MAIN_SHARED_BATCH | KIND_POPUP_SHARED_BATCH | KIND_GUEST_SHARED_BATCH => {
                OsrMessage::PaintBatch(paint::parse_shared_batch(
                    batch_surface(kind),
                    batch_header,
                    &payload,
                    #[cfg(unix)]
                    &mut fd,
                    &mut self.slots,
                )?)
            }
            KIND_MAIN_ACCEL | KIND_POPUP_ACCEL | KIND_GUEST_ACCEL => {
                let (mut frame, meta) = parse_accel_frame(kind, width, height, x, y, &payload)?;
                self.resources.attach(&mut frame, meta);
                OsrMessage::AccelFrame(frame)
            }
            #[cfg(any(windows, target_os = "linux"))]
            KIND_ACCEL_RETIRE => {
                self.resources.retire(parse_retired_resources(&payload)?);
                return Ok(None);
            }
            #[cfg(target_os = "linux")]
            KIND_ACCEL_DMABUF => {
                let (resource_id, dmabuf) =
                    parse_dmabuf_announcement(width, height, &payload, fd.take())?;
                self.resources.announce(resource_id, dmabuf);
                return Ok(None);
            }
            KIND_ACCEL_UNAVAILABLE => OsrMessage::AccelUnavailable,
            KIND_POPUP_HIDDEN => OsrMessage::PopupHidden,
            KIND_GUEST_HIDDEN => {
                OsrMessage::GuestHidden(String::from_utf8(payload).unwrap_or_default())
            }
            KIND_GUEST_ORDER => parse_guest_order(&payload),
            KIND_GUEST_CAPTURE_REQUESTED => parse_guest_capture(&payload)?,
            KIND_DRAGGABLE_REGIONS_CHANGED => {
                let (drag, exclusion) = parse_draggable_regions(&payload)?;
                OsrMessage::DraggableRegionsChanged { drag, exclusion }
            }
            KIND_CURSOR => OsrMessage::Cursor(parse_cursor(width, height, x, y, payload)?),
            KIND_CLOSE_REQUESTED => OsrMessage::CloseRequested,
            KIND_START_DRAG_REQUESTED => OsrMessage::StartDragRequested,
            KIND_MINIMIZE_REQUESTED => OsrMessage::MinimizeRequested,
            KIND_MAXIMIZE_REQUESTED => OsrMessage::MaximizeRequested,
            KIND_RESTORE_REQUESTED => OsrMessage::RestoreRequested,
            KIND_TOGGLE_MAXIMIZE_REQUESTED => OsrMessage::ToggleMaximizeRequested,
            KIND_FULLSCREEN_REQUESTED => OsrMessage::FullscreenRequested(true),
            KIND_EXIT_FULLSCREEN_REQUESTED => OsrMessage::FullscreenRequested(false),
            KIND_SHOW_REQUESTED => OsrMessage::ShowRequested,
            KIND_HIDE_REQUESTED => OsrMessage::HideRequested,
            KIND_FOCUS_REQUESTED => OsrMessage::FocusRequested(
                String::from_utf8(payload)
                    .ok()
                    .filter(|token| !token.trim().is_empty()),
            ),
            KIND_DRAG_STARTED => parse_drag_start(width, height, x, y, payload)?,
            KIND_DRAG_OPERATION => OsrMessage::DragOperation(width),
            KIND_CONTEXT_MENU => OsrMessage::ContextMenu {
                x,
                y,
                items: parse_json(&payload)?,
            },
            KIND_CONTEXT_MENU_DISMISSED => OsrMessage::ContextMenuDismissed,
            KIND_FILE_DIALOG => OsrMessage::FileDialog(parse_json(&payload)?),
            #[cfg(target_os = "linux")]
            KIND_PRINT_DIALOG => OsrMessage::PrintDialog(parse_json(&payload)?),
            #[cfg(target_os = "linux")]
            KIND_PRINT_JOB => OsrMessage::PrintJob(parse_json(&payload)?),
            KIND_PRINT_DOCUMENT => OsrMessage::PrintDocument(parse_json(&payload)?),
            KIND_MEDIA_PLAYING => OsrMessage::MediaPlaying(width != 0),
            KIND_MAIN_LOAD_STARTED => OsrMessage::MainLoadStarted,
            KIND_MAIN_LOAD_READY => OsrMessage::MainLoadReady,
            KIND_FATAL_ERROR => OsrMessage::FatalError(
                String::from_utf8(payload)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            ),
            KIND_IME_STATE_CHANGED => OsrMessage::ImeStateChanged(width),
            KIND_IME_CURSOR_AREA_CHANGED => OsrMessage::ImeCursorAreaChanged {
                x,
                y,
                width,
                height,
            },
            KIND_TOOLTIP_CHANGED => {
                OsrMessage::TooltipChanged(String::from_utf8(payload).unwrap_or_default())
            }
            KIND_IME_SURROUNDING_CHANGED => parse_ime_surrounding(&payload)?,
            KIND_BRIDGE_REQUEST => {
                OsrMessage::BridgeRequest(crate::bridge::frame::Frame::decode(&payload)?)
            }
            _ => return Err(invalid("unknown OSR message kind")),
        };
        Ok(Some(message))
    }
}

fn batch_surface(kind: u32) -> BatchSurface {
    match kind {
        KIND_MAIN_BATCH | KIND_MAIN_SHARED_BATCH => BatchSurface::Main,
        KIND_GUEST_BATCH | KIND_GUEST_SHARED_BATCH => BatchSurface::Guest,
        _ => BatchSurface::Popup,
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(all(test, unix))]
mod tests;
