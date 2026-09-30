mod accel;
mod header;
mod paint;
mod regions;
#[cfg(unix)]
mod shared_mem;

#[cfg(unix)]
pub(crate) use shared_mem::{PaintLease, PaintSlots};

use crate::osr::transport::IpcStream;
use std::io::{self, Read};

#[cfg(target_os = "macos")]
use crate::osr::accel::SurfaceRegistry;
use crate::osr::protocol::OsrMessage;
#[cfg(target_os = "macos")]
use std::sync::Arc;

#[cfg(target_os = "linux")]
use accel::{KIND_ACCEL_DMABUF, KIND_ACCEL_UNAVAILABLE, parse_dmabuf_announcement};
#[cfg(any(windows, target_os = "linux"))]
use accel::{KIND_ACCEL_RETIRE, parse_retired_resources};
use accel::{KIND_GUEST_ACCEL, KIND_MAIN_ACCEL, KIND_POPUP_ACCEL, parse_accel_frame};
use header::{read_header, read_i32, read_u32};
use paint::{BatchHeader, BatchSurface, parse_inline_batch};
use regions::{parse_draggable_regions, parse_file_drag_request};

pub(super) const HEADER_LEN: usize = 28;
pub(super) const MAGIC: &[u8; 4] = b"SAB1";
const MAX_SURFACE_DIMENSION: u32 = 16_384;
pub(super) const MAX_PAINT_BYTES: usize = 256 * 1024 * 1024;
const MAX_CONTROL_BYTES: usize = 64 * 1024 * 1024;
pub(super) const KIND_POPUP_HIDDEN: u32 = 3;
pub(super) const KIND_CURSOR: u32 = 4;
pub(super) const KIND_CLOSE_REQUESTED: u32 = 5;
pub(super) const KIND_START_DRAG_REQUESTED: u32 = 6;
pub(super) const KIND_MINIMIZE_REQUESTED: u32 = 7;
pub(super) const KIND_TOGGLE_MAXIMIZE_REQUESTED: u32 = 8;
pub(super) const KIND_SHOW_REQUESTED: u32 = 9;
pub(super) const KIND_HIDE_REQUESTED: u32 = 10;
pub(super) const KIND_FOCUS_REQUESTED: u32 = 11;
pub(super) const KIND_MAIN_BATCH: u32 = 12;
pub(super) const KIND_POPUP_BATCH: u32 = 13;
pub(super) const KIND_MAIN_SHARED_BATCH: u32 = 14;
pub(super) const KIND_POPUP_SHARED_BATCH: u32 = 15;
pub(super) const KIND_FILE_DRAG_REQUESTED: u32 = 16;
pub(super) const KIND_GUEST_BATCH: u32 = 18;
pub(super) const KIND_GUEST_SHARED_BATCH: u32 = 19;
pub(super) const KIND_GUEST_HIDDEN: u32 = 20;
pub(super) const KIND_DRAGGABLE_REGIONS_CHANGED: u32 = 21;
pub(super) const KIND_GUEST_CAPTURE_REQUESTED: u32 = 22;
pub(super) const KIND_BRIDGE_REQUEST: u32 = 23;
pub(super) const KIND_FULLSCREEN_REQUESTED: u32 = 27;
pub(super) const KIND_EXIT_FULLSCREEN_REQUESTED: u32 = 28;
pub(super) const KIND_MAIN_LOAD_STARTED: u32 = 29;
pub(super) const KIND_MAIN_LOAD_READY: u32 = 30;
pub(super) const KIND_IME_STATE_CHANGED: u32 = 31;
pub(super) const KIND_IME_CURSOR_AREA_CHANGED: u32 = 32;
pub(super) const KIND_TOOLTIP_CHANGED: u32 = 33;
pub(super) const KIND_IME_SURROUNDING_CHANGED: u32 = 34;
pub(super) const KIND_MAXIMIZE_REQUESTED: u32 = 35;
pub(super) const KIND_RESTORE_REQUESTED: u32 = 36;
pub(super) const KIND_FATAL_ERROR: u32 = 37;
pub(super) const KIND_HOST_HELLO: u32 = 38;
const MAX_HELLO_BYTES: usize = 64;
pub(super) const BATCH_ENTRY_LEN: usize = 28;

pub(crate) struct WireReader {
    stream: IpcStream,
    #[cfg(unix)]
    slots: PaintSlots,
    #[cfg(target_os = "macos")]
    surfaces: Arc<SurfaceRegistry>,
    #[cfg(target_os = "linux")]
    dmabufs: crate::osr::accel::Dmabufs,
    #[cfg(windows)]
    shared_handles: crate::osr::accel::SharedHandles,
}

impl WireReader {
    #[cfg(unix)]
    pub(crate) fn new(
        stream: IpcStream,
        slots: PaintSlots,
        #[cfg(target_os = "macos")] surfaces: Arc<SurfaceRegistry>,
    ) -> Self {
        Self {
            stream,
            slots,
            #[cfg(target_os = "macos")]
            surfaces,
            #[cfg(target_os = "linux")]
            dmabufs: Default::default(),
        }
    }

    #[cfg(not(unix))]
    pub(crate) fn new(stream: IpcStream) -> Self {
        Self {
            stream,
            shared_handles: Default::default(),
        }
    }

    pub(crate) fn read_host_protocol(&mut self) -> io::Result<Option<String>> {
        #[cfg(unix)]
        let Some((header, _)) = read_header(&mut self.stream)? else {
            return Ok(None);
        };
        #[cfg(not(unix))]
        let Some(header) = read_header(&mut self.stream)? else {
            return Ok(None);
        };
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

    #[cfg(windows)]
    fn shared_resource(
        &mut self,
        resource_id: u64,
        shared_handle: u64,
    ) -> Option<crate::osr::accel::SharedResource> {
        self.shared_handles.resolve(resource_id, shared_handle)
    }

    #[cfg(target_os = "macos")]
    fn shared_resource(
        &mut self,
        resource_id: u64,
        _shared_handle: u64,
    ) -> Option<crate::osr::accel::SharedResource> {
        self.surfaces.surface(resource_id)
    }

    #[cfg(target_os = "linux")]
    fn shared_resource(
        &mut self,
        resource_id: u64,
        _shared_handle: u64,
    ) -> Option<crate::osr::accel::SharedResource> {
        self.dmabufs.resolve(resource_id)
    }

    pub(crate) fn read(&mut self) -> io::Result<Option<OsrMessage>> {
        let reader = &mut self.stream;
        #[cfg(unix)]
        let Some((header, mut fd)) = read_header(reader)? else {
            return Ok(None);
        };
        #[cfg(not(unix))]
        let Some(header) = read_header(reader)? else {
            return Ok(None);
        };
        if &header[0..4] != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid OSR message magic",
            ));
        }

        let kind = read_u32(&header[4..8]);
        let width = read_u32(&header[8..12]);
        let height = read_u32(&header[12..16]);
        let x = read_i32(&header[16..20]);
        let y = read_i32(&header[20..24]);
        let payload_len = read_u32(&header[24..28]) as usize;
        if width > MAX_SURFACE_DIMENSION || height > MAX_SURFACE_DIMENSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OSR surface dimensions exceed the protocol limit",
            ));
        }
        if is_paint_kind(kind)
            && (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .is_none_or(|bytes| bytes > MAX_PAINT_BYTES)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OSR surface exceeds the protocol byte limit",
            ));
        }
        let payload_limit = if is_paint_kind(kind) {
            MAX_PAINT_BYTES
        } else {
            MAX_CONTROL_BYTES
        };
        if payload_len > payload_limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OSR payload exceeds the protocol limit",
            ));
        }
        let mut payload = vec![0_u8; payload_len];
        if payload_len > 0 {
            reader.read_exact(&mut payload)?;
        }

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
            #[cfg(unix)]
            KIND_MAIN_SHARED_BATCH | KIND_POPUP_SHARED_BATCH | KIND_GUEST_SHARED_BATCH => {
                OsrMessage::PaintBatch(paint::parse_shared_batch(
                    batch_surface(kind),
                    batch_header,
                    &payload,
                    &mut fd,
                    &mut self.slots,
                )?)
            }
            KIND_POPUP_HIDDEN => OsrMessage::PopupHidden,
            KIND_GUEST_HIDDEN => {
                OsrMessage::GuestHidden(String::from_utf8(payload).unwrap_or_default())
            }
            KIND_GUEST_CAPTURE_REQUESTED => {
                let mut parts = payload.splitn(3, |byte| *byte == 0);
                let browser_id = String::from_utf8(parts.next().unwrap_or_default().to_vec())
                    .unwrap_or_default();
                let request_id = String::from_utf8(parts.next().unwrap_or_default().to_vec())
                    .unwrap_or_default();
                let guest_id = String::from_utf8(parts.next().unwrap_or_default().to_vec())
                    .unwrap_or_default();
                if browser_id.is_empty() || request_id.is_empty() || guest_id.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid guest capture request",
                    ));
                }
                OsrMessage::GuestCaptureRequested {
                    browser_id,
                    request_id,
                    guest_id,
                }
            }
            KIND_DRAGGABLE_REGIONS_CHANGED => {
                let (drag, exclusion) = parse_draggable_regions(&payload)?;
                OsrMessage::DraggableRegionsChanged { drag, exclusion }
            }
            KIND_CURSOR => OsrMessage::Cursor(String::from_utf8(payload).unwrap_or_default()),
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
            KIND_FILE_DRAG_REQUESTED => match parse_file_drag_request(&payload) {
                Some(request) => OsrMessage::FileDragRequested(request),
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid file drag request payload",
                    ));
                }
            },
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
            KIND_MAIN_ACCEL | KIND_POPUP_ACCEL | KIND_GUEST_ACCEL => {
                let (frame, shared_handle) =
                    parse_accel_frame(kind, width, height, x, y, &payload)?;
                OsrMessage::AccelFrame(crate::osr::protocol::OsrAccelFrame {
                    resource: self.shared_resource(frame.resource_id, shared_handle),
                    ..frame
                })
            }
            #[cfg(windows)]
            KIND_ACCEL_RETIRE => {
                self.shared_handles
                    .retire(parse_retired_resources(&payload)?);
                return self.read();
            }
            #[cfg(target_os = "linux")]
            KIND_ACCEL_RETIRE => {
                self.dmabufs.retire(parse_retired_resources(&payload)?);
                return self.read();
            }
            #[cfg(target_os = "linux")]
            KIND_ACCEL_DMABUF => {
                let (resource_id, dmabuf) =
                    parse_dmabuf_announcement(width, height, &payload, fd.take())?;
                self.dmabufs.announce(resource_id, dmabuf);
                return self.read();
            }
            #[cfg(target_os = "linux")]
            KIND_ACCEL_UNAVAILABLE => OsrMessage::AccelUnavailable,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unknown OSR message kind",
                ));
            }
        };
        Ok(Some(message))
    }
}

fn parse_ime_surrounding(payload: &[u8]) -> io::Result<OsrMessage> {
    let value: serde_json::Value = serde_json::from_slice(payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let text = value
        .get("text")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let number = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0)
    };
    Ok(OsrMessage::ImeSurroundingChanged {
        text,
        cursor_utf16: number("cursor"),
        anchor_utf16: number("anchor"),
        base_utf16: number("base"),
    })
}

fn batch_surface(kind: u32) -> BatchSurface {
    match kind {
        KIND_MAIN_BATCH | KIND_MAIN_SHARED_BATCH => BatchSurface::Main,
        KIND_GUEST_BATCH | KIND_GUEST_SHARED_BATCH => BatchSurface::Guest,
        _ => BatchSurface::Popup,
    }
}

fn is_paint_kind(kind: u32) -> bool {
    if matches!(kind, KIND_MAIN_ACCEL | KIND_POPUP_ACCEL | KIND_GUEST_ACCEL) {
        return true;
    }
    matches!(
        kind,
        KIND_MAIN_BATCH
            | KIND_POPUP_BATCH
            | KIND_MAIN_SHARED_BATCH
            | KIND_POPUP_SHARED_BATCH
            | KIND_GUEST_BATCH
            | KIND_GUEST_SHARED_BATCH
    )
}

#[cfg(all(test, unix))]
mod tests;
