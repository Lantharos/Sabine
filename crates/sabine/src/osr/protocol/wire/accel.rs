use super::header::{read_i32, read_u32, read_u64};
use super::kinds::{KIND_GUEST_ACCEL, KIND_MAIN_ACCEL, KIND_POPUP_ACCEL};
use super::paint::split_guest_payload;
use crate::osr::protocol::{OsrAccelFrame, OsrSurface};
use std::io;

const META_LEN: usize = 4 + 4 + 4 + 4 + 4 + 8 + 4 + 8 + 8;

/// The texture handle a frame hands over with it on Windows, opened in this
/// process by the browser host, or 0 when the frame names one sent before.
#[cfg(windows)]
pub(super) fn announced_texture(meta: &[u8]) -> u64 {
    read_u64(&meta[32..40])
}

/// Parses an accelerated frame, returning its metadata too for what each
/// platform reads from it alone.
pub(super) fn parse_accel_frame(
    kind: u32,
    coded_width: u32,
    coded_height: u32,
    x: i32,
    y: i32,
    payload: &[u8],
) -> io::Result<(OsrAccelFrame, &[u8])> {
    let (surface, rest_start) = match kind {
        KIND_GUEST_ACCEL => {
            let (guest_id, rest) = split_guest_payload(payload)?;
            (OsrSurface::Guest(guest_id), rest)
        }
        KIND_MAIN_ACCEL => (OsrSurface::Main, 0),
        KIND_POPUP_ACCEL => (OsrSurface::Popup, 0),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unknown accelerated OSR kind",
            ));
        }
    };

    let rest = &payload[rest_start..];
    if rest.len() != META_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid accelerated OSR metadata length",
        ));
    }

    let visible_x = read_i32(&rest[4..8]);
    let visible_y = read_i32(&rest[8..12]);
    let visible_width = read_u32(&rest[12..16]);
    let visible_height = read_u32(&rest[16..20]);
    if visible_x < 0
        || visible_y < 0
        || visible_width == 0
        || visible_height == 0
        || (visible_x as u32).saturating_add(visible_width) > coded_width
        || (visible_y as u32).saturating_add(visible_height) > coded_height
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid accelerated OSR visible rect",
        ));
    }
    let frame = OsrAccelFrame {
        surface,
        coded_width,
        coded_height,
        visible_x: visible_x as u32,
        visible_y: visible_y as u32,
        visible_width,
        visible_height,
        x,
        y,
        format: read_u32(&rest[0..4]),
        resource_id: read_u64(&rest[20..28]),
        resource_slot: read_u32(&rest[28..32]),
        slot_token: read_u64(&rest[40..48]),
        resource: None,
    };
    Ok((frame, rest))
}

#[cfg(any(windows, target_os = "linux"))]
pub(super) fn parse_retired_resources(payload: &[u8]) -> io::Result<Vec<u64>> {
    let (resources, []) = payload.as_chunks::<8>() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid accelerated resource retirement",
        ));
    };
    Ok(resources.iter().copied().map(u64::from_le_bytes).collect())
}

#[cfg(target_os = "linux")]
pub(super) fn parse_dmabuf_announcement(
    width: u32,
    height: u32,
    payload: &[u8],
    fd: Option<i32>,
) -> io::Result<(u64, crate::osr::accel::Dmabuf)> {
    use std::os::fd::{FromRawFd, OwnedFd};

    let fd = fd.map(|fd| unsafe { OwnedFd::from_raw_fd(fd) });
    let (Some(fd), 24) = (fd, payload.len()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid dma-buf announcement",
        ));
    };
    Ok((
        read_u64(&payload[0..8]),
        crate::osr::accel::Dmabuf {
            fd,
            width,
            height,
            modifier: read_u64(&payload[8..16]),
            stride: read_u32(&payload[16..20]),
            offset: read_u32(&payload[20..24]),
        },
    ))
}
