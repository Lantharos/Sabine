// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// A window region clips everything DWM composes for the window, including
// its DirectComposition content and backdrop, and the window stops receiving
// input outside it. The system owns a region once SetWindowRgn accepts it.

use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{ExtCreateRegion, RDH_RECTANGLES, RGNDATA, RGNDATAHEADER, SetWindowRgn},
};

use crate::WindowRegionRect;

/// Limits the window to `rects`, in physical pixels, or lifts the limit.
pub(super) fn set_window_shape(hwnd: HWND, rects: Option<&[RECT]>) {
    let region = rects.map(create_region);
    unsafe {
        SetWindowRgn(hwnd, region, true);
    }
}

pub(super) fn physical_rects(rects: &[WindowRegionRect], scale: f64) -> Vec<RECT> {
    let physical = |value: i32| (f64::from(value) * scale).round() as i32;
    rects
        .iter()
        .map(|rect| RECT {
            left: physical(rect.x),
            top: physical(rect.y),
            right: physical(rect.x + rect.width),
            bottom: physical(rect.y + rect.height),
        })
        .collect()
}

fn create_region(rects: &[RECT]) -> windows::Win32::Graphics::Gdi::HRGN {
    let header_words = size_of::<RGNDATAHEADER>() / size_of::<u32>();
    let rect_words = size_of::<RECT>() / size_of::<u32>();
    let mut data = vec![0_u32; header_words + rects.len() * rect_words];
    let bounds = rects.iter().fold(
        RECT {
            left: i32::MAX,
            top: i32::MAX,
            right: i32::MIN,
            bottom: i32::MIN,
        },
        |bounds, rect| RECT {
            left: bounds.left.min(rect.left),
            top: bounds.top.min(rect.top),
            right: bounds.right.max(rect.right),
            bottom: bounds.bottom.max(rect.bottom),
        },
    );
    let header = RGNDATAHEADER {
        dwSize: size_of::<RGNDATAHEADER>() as u32,
        iType: RDH_RECTANGLES,
        nCount: rects.len() as u32,
        nRgnSize: size_of_val(rects) as u32,
        rcBound: bounds,
    };
    unsafe {
        let start = data.as_mut_ptr();
        start.cast::<RGNDATAHEADER>().write(header);
        std::ptr::copy_nonoverlapping(
            rects.as_ptr(),
            start.add(header_words).cast::<RECT>(),
            rects.len(),
        );
        ExtCreateRegion(
            None,
            (data.len() * size_of::<u32>()) as u32,
            data.as_ptr().cast::<RGNDATA>(),
        )
    }
}
