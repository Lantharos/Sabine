use std::io;
use std::sync::Arc;

use super::BATCH_ENTRY_LEN;
use super::header::{read_i32, read_u32, read_u64};
#[cfg(unix)]
use super::{MAX_PAINT_BYTES, header::ReceivedFd, shared_mem::PaintSlots};
use crate::osr::protocol::{FrameBytes, OsrPaintBatch, OsrSurface, PaintRect};

#[derive(Clone, Copy)]
pub(super) enum BatchSurface {
    Main,
    Popup,
    Guest,
}

pub(super) struct BatchHeader {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) x: i32,
    pub(super) y: i32,
}

pub(super) fn parse_inline_batch(
    target: BatchSurface,
    header: BatchHeader,
    payload: Vec<u8>,
) -> io::Result<OsrPaintBatch> {
    let source = Arc::new(payload);
    let (surface, metadata_start) = batch_surface(target, &source)?;
    let entries = batch_entries(&source[metadata_start..])?;
    let blob_start = metadata_start + 4 + entries.len();
    let rects = parse_rects(
        entries,
        source.len(),
        |offset| {
            blob_start.checked_add(offset).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "OSR paint rect offset overflow")
            })
        },
        |range| FrameBytes::Inline {
            source: Arc::clone(&source),
            range,
        },
    )?;
    Ok(batch(surface, header, rects))
}

#[cfg(unix)]
pub(super) fn parse_shared_batch(
    target: BatchSurface,
    header: BatchHeader,
    payload: &[u8],
    received_fd: &mut ReceivedFd,
    slots: &mut PaintSlots,
) -> io::Result<OsrPaintBatch> {
    let (surface, metadata_start) = batch_surface(target, payload)?;
    let slot_metadata = payload
        .get(metadata_start..metadata_start + 8)
        .ok_or_else(|| invalid("truncated OSR shared paint slot"))?;
    let lease = slots.lease(
        read_u32(&slot_metadata[0..4]),
        read_u32(&slot_metadata[4..8]),
        received_fd.take(),
        MAX_PAINT_BYTES,
    )?;
    let entries = batch_entries(&payload[metadata_start + 8..])?;
    let rects = parse_rects(entries, lease.as_slice().len(), Ok, |range| {
        FrameBytes::Shared {
            source: Arc::clone(&lease),
            range,
        }
    })?;
    Ok(batch(surface, header, rects))
}

fn batch_surface(target: BatchSurface, payload: &[u8]) -> io::Result<(OsrSurface, usize)> {
    Ok(match target {
        BatchSurface::Main => (OsrSurface::Main, 0),
        BatchSurface::Popup => (OsrSurface::Popup, 0),
        BatchSurface::Guest => {
            let (guest_id, rest_start) = split_guest_payload(payload)?;
            (OsrSurface::Guest(guest_id), rest_start)
        }
    })
}

fn batch_entries(metadata: &[u8]) -> io::Result<&[u8]> {
    let count = super::regions::payload_count(metadata)?;
    let entries_len = count
        .checked_mul(BATCH_ENTRY_LEN)
        .ok_or_else(|| invalid("OSR paint batch entry count overflow"))?;
    metadata
        .get(4..4 + entries_len)
        .ok_or_else(|| invalid("truncated OSR paint batch"))
}

fn parse_rects(
    entries: &[u8],
    source_len: usize,
    source_offset: impl Fn(usize) -> io::Result<usize>,
    bytes: impl Fn(std::ops::Range<usize>) -> FrameBytes,
) -> io::Result<Vec<PaintRect>> {
    entries
        .chunks_exact(BATCH_ENTRY_LEN)
        .map(|entry| {
            let width = read_u32(&entry[8..12]);
            let height = read_u32(&entry[12..16]);
            let len = read_u32(&entry[24..28]) as usize;
            let expected_len = (width as usize)
                .checked_mul(height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| invalid("OSR paint rect size overflow"))?;
            if len != expected_len {
                return Err(invalid("invalid OSR paint rect byte length"));
            }
            let start = source_offset(read_u64(&entry[16..24]) as usize)?;
            let end = start
                .checked_add(len)
                .filter(|end| *end <= source_len)
                .ok_or_else(|| invalid("truncated OSR paint rect bytes"))?;
            Ok(PaintRect {
                width,
                height,
                x: read_i32(&entry[0..4]),
                y: read_i32(&entry[4..8]),
                bytes: bytes(start..end),
            })
        })
        .collect()
}

fn batch(surface: OsrSurface, header: BatchHeader, rects: Vec<PaintRect>) -> OsrPaintBatch {
    OsrPaintBatch {
        surface,
        width: header.width,
        height: header.height,
        x: header.x,
        y: header.y,
        rects,
    }
}

pub(super) fn split_guest_payload(payload: &[u8]) -> io::Result<(String, usize)> {
    let id_len = payload
        .get(0..2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]) as usize)
        .ok_or_else(|| invalid("guest OSR payload missing id length"))?;
    let id = payload
        .get(2..2 + id_len)
        .ok_or_else(|| invalid("guest OSR payload missing id bytes"))?;
    if id.is_empty() {
        return Err(invalid("guest OSR payload has empty id"));
    }
    Ok((String::from_utf8_lossy(id).into_owned(), 2 + id_len))
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
