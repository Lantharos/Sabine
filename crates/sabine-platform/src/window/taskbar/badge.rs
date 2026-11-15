//! The overlay icon: the badge's label in white on a red disc, at the size
//! the taskbar shows overlays at on the window's display. GDI draws the
//! label's antialiased coverage into a buffer of its own, which then sets
//! the label's colour over the disc.

use std::{ffi::c_void, ptr};

use windows::Win32::Foundation::{COLORREF, HWND, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateBitmap,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS, DT_CENTER,
    DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, FW_BOLD, GdiFlush,
    GetTextExtentPoint32W, HBITMAP, HDC, HFONT, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};
use windows::core::w;

const DISC: [f32; 3] = [0xd1 as f32, 0x34 as f32, 0x38 as f32];
const LONGEST_LABEL: usize = 3;

pub(super) fn icon(window: HWND, label: &str) -> Option<HICON> {
    let size = (16 * unsafe { GetDpiForWindow(window) }.max(96) / 96) as i32;
    let coverage = label_coverage(size, &shortened(label))?;
    create_icon(size, &disc_with_label(size, &coverage))
}

/// Counts past 99 show as `99+`, and text keeps its first characters.
fn shortened(label: &str) -> String {
    match label.parse::<u64>() {
        Ok(count) if count > 99 => "99+".to_string(),
        _ => label.chars().take(LONGEST_LABEL).collect(),
    }
}

/// How much of each pixel the label covers, from 0 to 255.
fn label_coverage(size: i32, label: &str) -> Option<Vec<u8>> {
    let mut text = label.encode_utf16().collect::<Vec<_>>();
    unsafe {
        let dc = CreateCompatibleDC(None);
        let (bitmap, bits) = bitmap(Some(dc), size)?;
        let previous = SelectObject(dc, bitmap.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00ff_ffff));
        let font = fitted_font(dc, size, &text);
        let mut bounds = RECT {
            left: 0,
            top: 0,
            right: size,
            bottom: size,
        };
        DrawTextW(
            dc,
            &mut text,
            &mut bounds,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        let _ = GdiFlush();
        let pixels = (size * size) as usize;
        let coverage = std::slice::from_raw_parts(bits.cast::<[u8; 4]>(), pixels)
            .iter()
            .map(|pixel| pixel[1])
            .collect();
        SelectObject(dc, previous);
        if let Some(font) = font {
            let _ = DeleteObject(font.into());
        }
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        Some(coverage)
    }
}

/// The largest bold Segoe UI the label fits the disc in, left selected.
unsafe fn fitted_font(dc: HDC, size: i32, text: &[u16]) -> Option<HFONT> {
    let widest = size * 9 / 10;
    (size / 3..=size * 3 / 4).rev().find_map(|height| unsafe {
        let font = CreateFontW(
            -height,
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            0,
            w!("Segoe UI"),
        );
        let previous = SelectObject(dc, font.into());
        let mut extent = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, text, &mut extent);
        if extent.cx <= widest || height == size / 3 {
            return Some(font);
        }
        SelectObject(dc, previous);
        let _ = DeleteObject(font.into());
        None
    })
}

/// Straight BGRA pixels of the disc, edges smoothed, with the label in white.
fn disc_with_label(size: i32, coverage: &[u8]) -> Vec<u8> {
    let radius = size as f32 / 2.0;
    let mut pixels = Vec::with_capacity(coverage.len() * 4);
    for (index, &covered) in coverage.iter().enumerate() {
        let x = (index as i32 % size) as f32 + 0.5 - radius;
        let y = (index as i32 / size) as f32 + 0.5 - radius;
        let disc = (radius - (x * x + y * y).sqrt() + 0.5).clamp(0.0, 1.0);
        let label = f32::from(covered) / 255.0;
        let [red, green, blue] = DISC.map(|channel| channel + (255.0 - channel) * label);
        pixels.extend([blue, green, red, disc * 255.0].map(|value| value.round() as u8));
    }
    pixels
}

fn create_icon(size: i32, pixels: &[u8]) -> Option<HICON> {
    unsafe {
        let (color, bits) = bitmap(None, size)?;
        ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
        let mask_row = ((size + 15) / 16 * 2) as usize;
        let mask_bits = vec![0u8; mask_row * size as usize];
        let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));
        let icon = CreateIconIndirect(&ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        })
        .ok();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

/// A top-down 32-bit bitmap whose pixels start out transparent black.
unsafe fn bitmap(dc: Option<HDC>, size: i32) -> Option<(HBITMAP, *mut c_void)> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut bits = ptr::null_mut();
    let bitmap = unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?;
    Some((bitmap, bits))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_labels_shorten() {
        assert_eq!(shortened("120"), "99+");
        assert_eq!(shortened("42"), "42");
        assert_eq!(shortened("new!"), "new");
    }
}
