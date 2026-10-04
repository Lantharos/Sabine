mod bgra;
#[cfg(windows)]
mod composition;
mod display_list;
mod gpu;
mod pixel_rect;
pub(crate) mod raster_text;
mod rect_pipeline;

pub(crate) use bgra::{BgraImage, BgraRect};
#[cfg(windows)]
pub(crate) use composition::Composition;
pub use display_list::{
    DisplayCommand, DisplayList, ImageCommand, RectCommand, RoundedRectCommand, TextCommand,
};
#[cfg(any(windows, target_os = "macos"))]
pub(crate) use gpu::ExternalSlot;
pub use gpu::{GpuRenderer, RendererError};
pub(crate) use pixel_rect::PixelRect;

/// Chromium accepts device scale factors down to a quarter; the window host
/// converts between physical and logical pixels with the same bound so both
/// sides agree on the page size.
const MIN_SCALE_FACTOR: f64 = 0.25;

pub(crate) fn effective_scale(scale: f64) -> f64 {
    scale.max(MIN_SCALE_FACTOR)
}
