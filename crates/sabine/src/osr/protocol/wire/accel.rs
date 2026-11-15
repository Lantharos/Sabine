use super::header::{read_i32, read_u32, read_u64};
use super::kinds::{KIND_GUEST_ACCEL, KIND_MAIN_ACCEL, KIND_POPUP_ACCEL};
use super::paint::split_guest_payload;
use crate::osr::protocol::{OsrAccelFrame, OsrSurface, TextureRect};
use std::io;

const META_LEN: usize = 4 + 4 + 4 + 4 + 4 + 8 + 4 + 8 + 8 + 4 + 4 + 4 + 4 + 8;

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
        _ => return Err(invalid("unknown accelerated OSR kind")),
    };

    let rest = &payload[rest_start..];
    if rest.len() != META_LEN {
        return Err(invalid("invalid accelerated OSR metadata length"));
    }

    let coded = (coded_width, coded_height);
    let visible = rect_within(&rest[4..20], coded)
        .filter(|visible| visible.width != 0 && visible.height != 0)
        .ok_or_else(|| invalid("invalid accelerated OSR visible rect"))?;
    #[cfg(target_os = "linux")]
    let damage = rect_within(&rest[48..64], coded)
        .ok_or_else(|| invalid("invalid accelerated OSR damage rect"))?;
    let frame = OsrAccelFrame {
        surface,
        coded_width,
        coded_height,
        visible_x: visible.x,
        visible_y: visible.y,
        visible_width: visible.width,
        visible_height: visible.height,
        #[cfg(target_os = "linux")]
        damage,
        #[cfg(target_os = "linux")]
        sequence: read_u64(&rest[64..72]),
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

/// Reads an `x, y, width, height` rect that has to lie within `size`.
fn rect_within(bytes: &[u8], size: (u32, u32)) -> Option<TextureRect> {
    let x = u32::try_from(read_i32(&bytes[0..4])).ok()?;
    let y = u32::try_from(read_i32(&bytes[4..8])).ok()?;
    let width = read_u32(&bytes[8..12]);
    let height = read_u32(&bytes[12..16]);
    (x.checked_add(width)? <= size.0 && y.checked_add(height)? <= size.1).then_some(TextureRect {
        x,
        y,
        width,
        height,
    })
}

#[cfg(any(windows, target_os = "linux"))]
pub(super) fn parse_retired_resources(payload: &[u8]) -> io::Result<Vec<u64>> {
    let (resources, []) = payload.as_chunks::<8>() else {
        return Err(invalid("invalid accelerated resource retirement"));
    };
    Ok(resources.iter().copied().map(u64::from_le_bytes).collect())
}

#[cfg(target_os = "linux")]
pub(super) fn parse_dmabuf_announcement(
    width: u32,
    height: u32,
    payload: &[u8],
    fd: Option<std::os::fd::OwnedFd>,
) -> io::Result<(u64, crate::osr::accel::Dmabuf)> {
    let (Some(fd), 24) = (fd, payload.len()) else {
        return Err(invalid("invalid dma-buf announcement"));
    };
    let dmabuf = crate::osr::accel::Dmabuf {
        fd,
        width,
        height,
        modifier: read_u64(&payload[8..16]),
        stride: read_u32(&payload[16..20]),
        offset: read_u32(&payload[20..24]),
    };
    if !dmabuf_fits(&dmabuf) {
        return Err(invalid("dma-buf layout does not fit its buffer"));
    }
    Ok((read_u64(&payload[0..8]), dmabuf))
}

/// Whether `dmabuf` names a frame its buffer holds: rows of at least four
/// bytes per pixel, all of them before the buffer's end.
#[cfg(target_os = "linux")]
fn dmabuf_fits(dmabuf: &crate::osr::accel::Dmabuf) -> bool {
    use std::os::fd::AsRawFd;

    let end = unsafe { libc::lseek(dmabuf.fd.as_raw_fd(), 0, libc::SEEK_END) };
    let Ok(size) = u64::try_from(end) else {
        return false;
    };
    let stride = u64::from(dmabuf.stride);
    dmabuf.width != 0
        && dmabuf.height != 0
        && stride >= u64::from(dmabuf.width) * 4
        && u64::from(dmabuf.offset) + stride * u64::from(dmabuf.height) <= size
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
