// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The clipboard is one global lock: OpenClipboard fails while another app
// holds it, so opening retries briefly. Data handed to SetClipboardData
// belongs to the system afterwards and must not be freed; data from
// GetClipboardData stays owned by the clipboard and is only read while it is
// open.

mod formats;

use std::{thread, time::Duration};

use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
            SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
    },
};

use super::ClipboardContent;
use super::content::read_plan;
use super::worker::Pasteboard;
use formats::Format;

pub(super) struct WindowsClipboard;

impl Pasteboard for WindowsClipboard {
    fn read(types: Option<&[String]>) -> Result<ClipboardContent, String> {
        let _open = OpenedClipboard::open()?;
        let offered = offered_formats();
        let mimes = offered
            .iter()
            .map(|(mime, _)| mime.clone())
            .collect::<Vec<_>>();
        let mut content = ClipboardContent::default();
        for (source, mime) in read_plan(&mimes, types) {
            let Some((_, format)) = offered.iter().find(|(offered, _)| *offered == source) else {
                continue;
            };
            if let Some(bytes) = global_bytes(format.id()).and_then(|bytes| format.to_mime(&bytes))
            {
                content.push(mime, bytes);
            }
        }
        Ok(content)
    }

    fn write(content: &ClipboardContent) -> Result<(), String> {
        let _open = OpenedClipboard::open()?;
        unsafe { EmptyClipboard() }.map_err(|error| error.to_string())?;
        for (mime, bytes) in content.items() {
            for (format, data) in Format::for_mime(mime, bytes) {
                set_global_bytes(format.id(), &data)?;
            }
        }
        Ok(())
    }
}

/// Every format on the clipboard Sabine can name as a MIME type, the
/// preferred representation of each type first.
fn offered_formats() -> Vec<(String, Format)> {
    let mut offered: Vec<(String, Format)> = Vec::new();
    let mut id = 0;
    loop {
        id = unsafe { EnumClipboardFormats(id) };
        if id == 0 {
            break;
        }
        if let Some(format) = Format::from_id(id) {
            let mime = format.mime();
            match offered.iter_mut().find(|(offered, _)| *offered == mime) {
                Some(existing) if format.preferred_over(&existing.1) => existing.1 = format,
                Some(_) => {}
                None => offered.push((mime, format)),
            }
        }
    }
    offered
}

struct OpenedClipboard;

impl OpenedClipboard {
    fn open() -> Result<Self, String> {
        let mut attempts = 0;
        loop {
            match unsafe { OpenClipboard(None) } {
                Ok(()) => return Ok(Self),
                Err(_) if attempts < 10 => {
                    attempts += 1;
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(format!("The clipboard is busy: {error}")),
            }
        }
    }
}

impl Drop for OpenedClipboard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn global_bytes(format: u32) -> Option<Vec<u8>> {
    let handle = unsafe { GetClipboardData(format) }.ok()?;
    let global = HGLOBAL(handle.0);
    unsafe {
        let data = GlobalLock(global);
        if data.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(data.cast::<u8>(), GlobalSize(global)).to_vec();
        let _ = GlobalUnlock(global);
        Some(bytes)
    }
}

fn set_global_bytes(format: u32, bytes: &[u8]) -> Result<(), String> {
    unsafe {
        let global =
            GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).map_err(|error| error.to_string())?;
        let data = GlobalLock(global);
        if data.is_null() {
            let _ = GlobalFree(Some(global));
            return Err("Could not lock clipboard memory".to_string());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(global);
        if let Err(error) = SetClipboardData(format, Some(HANDLE(global.0))) {
            let _ = GlobalFree(Some(global));
            return Err(error.to_string());
        }
    }
    Ok(())
}
