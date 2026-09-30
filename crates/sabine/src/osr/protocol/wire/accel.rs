use super::header::{read_i32, read_u32, read_u64};
use super::paint::split_guest_payload;
use crate::osr::protocol::{OsrAccelFrame, OsrSurface};
use std::io;

pub(super) const KIND_MAIN_ACCEL: u32 = 24;
pub(super) const KIND_POPUP_ACCEL: u32 = 25;
pub(super) const KIND_GUEST_ACCEL: u32 = 26;
#[cfg(any(windows, target_os = "linux"))]
pub(super) const KIND_ACCEL_RETIRE: u32 = 39;
#[cfg(target_os = "linux")]
pub(super) const KIND_ACCEL_DMABUF: u32 = 40;
#[cfg(target_os = "linux")]
pub(super) const KIND_ACCEL_UNAVAILABLE: u32 = 41;

const META_LEN: usize = 4 + 4 + 4 + 4 + 4 + 8 + 4 + 8 + 8;

pub(super) fn parse_accel_frame(
    kind: u32,
    coded_width: u32,
    coded_height: u32,
    x: i32,
    y: i32,
    payload: &[u8],
) -> io::Result<(OsrAccelFrame, u64)> {
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

    let format = read_u32(&rest[0..4]);
    let visible_x = read_i32(&rest[4..8]);
    let visible_y = read_i32(&rest[8..12]);
    let visible_width = read_u32(&rest[12..16]);
    let visible_height = read_u32(&rest[16..20]);
    let resource_id = read_u64(&rest[20..28]);
    let resource_slot = read_u32(&rest[28..32]);
    let shared_handle = read_u64(&rest[32..40]);
    let slot_token = read_u64(&rest[40..48]);
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
        format,
        resource_id,
        resource_slot,
        slot_token,
        resource: None,
    };
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

    Ok((frame, shared_handle))
}

#[cfg(any(windows, target_os = "linux"))]
pub(super) fn parse_retired_resources(payload: &[u8]) -> io::Result<Vec<u64>> {
    if !payload.len().is_multiple_of(8) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid accelerated resource retirement",
        ));
    }
    Ok(payload.chunks_exact(8).map(read_u64).collect())
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
