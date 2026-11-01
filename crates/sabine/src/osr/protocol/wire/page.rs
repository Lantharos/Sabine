use std::io;

use super::header::read_u32;
use crate::osr::protocol::{CursorImage, DragContent, DragImage, OsrMessage, PageCursorMessage};

const CEF_CURSOR_CUSTOM: u32 = 45;

pub(super) fn parse_cursor(
    width: u32,
    height: u32,
    hotspot_x: i32,
    hotspot_y: i32,
    mut payload: Vec<u8>,
) -> io::Result<PageCursorMessage> {
    let kind = payload
        .get(0..4)
        .map(read_u32)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing cursor type"))?;
    if kind != CEF_CURSOR_CUSTOM {
        return Ok(PageCursorMessage::Named(kind));
    }
    let size = |value: u32| u16::try_from(value).ok().filter(|value| *value > 0);
    let hotspot = |value: i32| u16::try_from(value).ok();
    let (Some(width), Some(height), Some(hotspot_x), Some(hotspot_y)) = (
        size(width),
        size(height),
        hotspot(hotspot_x),
        hotspot(hotspot_y),
    ) else {
        return Ok(PageCursorMessage::Named(kind));
    };
    let mut rgba = payload.split_off(4);
    if rgba.len() != usize::from(width) * usize::from(height) * 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cursor image does not match its size",
        ));
    }
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(PageCursorMessage::Custom(CursorImage {
        rgba,
        width,
        height,
        hotspot_x,
        hotspot_y,
    }))
}

pub(super) fn parse_drag_start(
    width: u32,
    height: u32,
    hotspot_x: i32,
    hotspot_y: i32,
    mut payload: Vec<u8>,
) -> io::Result<OsrMessage> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct DragStart {
        #[serde(flatten)]
        content: DragContent,
        operations: u32,
        image_scale: f32,
    }
    let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
    let content_end = payload
        .get(0..4)
        .map(|length| 4 + read_u32(length) as usize)
        .filter(|end| *end <= payload.len())
        .ok_or_else(|| invalid("truncated drag content"))?;
    let rgba = payload.split_off(content_end);
    let start: DragStart = serde_json::from_slice(&payload[4..])
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(invalid("drag image does not match its size"));
    }
    let image = (!rgba.is_empty()).then_some(DragImage {
        rgba,
        width,
        height,
        hotspot_x,
        hotspot_y,
        scale: start.image_scale,
    });
    Ok(OsrMessage::DragStarted {
        content: start.content,
        operations: start.operations,
        image,
    })
}

pub(super) fn parse_ime_surrounding(payload: &[u8]) -> io::Result<OsrMessage> {
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

pub(super) fn parse_guest_capture(payload: &[u8]) -> io::Result<OsrMessage> {
    let mut parts = payload
        .splitn(3, |byte| *byte == 0)
        .map(|part| String::from_utf8(part.to_vec()).unwrap_or_default());
    let mut next = || parts.next().unwrap_or_default();
    let (browser_id, request_id, guest_id) = (next(), next(), next());
    if browser_id.is_empty() || request_id.is_empty() || guest_id.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid guest capture request",
        ));
    }
    Ok(OsrMessage::GuestCaptureRequested {
        browser_id,
        request_id,
        guest_id,
    })
}

/// The guests' ids, bottom first, each ended by a NUL byte.
pub(super) fn parse_guest_order(payload: &[u8]) -> OsrMessage {
    OsrMessage::GuestOrder(
        payload
            .split(|byte| *byte == 0)
            .filter(|id| !id.is_empty())
            .map(|id| std::sync::Arc::from(String::from_utf8_lossy(id).as_ref()))
            .collect(),
    )
}

pub(super) fn parse_json<T: serde::de::DeserializeOwned>(payload: &[u8]) -> io::Result<T> {
    serde_json::from_slice(payload)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
