use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_app_kit::{NSColor, NSCompositingOperation, NSImage, NSRectFillListUsingOperation};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::{WindowOptions, WindowRegion, WindowRegionRect};

/// The mask that limits the material to the blur region. Opaque content
/// hides the material anyway, so the opaque region is cut out of it too and
/// the window server does not blur behind it. `None` covers the whole window.
pub(super) fn material_mask(
    options: &WindowOptions,
    width: i32,
    height: i32,
    transparent_holes: &[WindowRegionRect],
) -> Option<Retained<NSImage>> {
    let regions = &options.regions;
    if regions.blur.is_none() && regions.opaque.is_none() && transparent_holes.is_empty() {
        return None;
    }
    let shown = rects(
        &regions
            .blur
            .clone()
            .unwrap_or_else(WindowRegion::adaptive_full)
            .resolved_rects(width, height),
    );
    let hidden = regions
        .opaque
        .as_ref()
        .map(|opaque| opaque.resolved_rects(width, height))
        .unwrap_or_default()
        .iter()
        .chain(transparent_holes)
        .map(rect)
        .collect::<Vec<_>>();
    let draw = RcBlock::new(move |_: NSRect| {
        NSColor::blackColor().set();
        unsafe {
            fill(&shown, NSCompositingOperation::Copy);
            fill(&hidden, NSCompositingOperation::Clear);
        }
        Bool::YES
    });
    Some(NSImage::imageWithSize_flipped_drawingHandler(
        NSSize::new(f64::from(width), f64::from(height)),
        true,
        &draw,
    ))
}

unsafe fn fill(rects: &[NSRect], operation: NSCompositingOperation) {
    unsafe {
        NSRectFillListUsingOperation(NonNull::from(rects).cast(), rects.len() as isize, operation);
    }
}

fn rects(rects: &[WindowRegionRect]) -> Vec<NSRect> {
    rects.iter().map(rect).collect()
}

fn rect(rect: &WindowRegionRect) -> NSRect {
    NSRect::new(
        NSPoint::new(f64::from(rect.x), f64::from(rect.y)),
        NSSize::new(f64::from(rect.width), f64::from(rect.height)),
    )
}
