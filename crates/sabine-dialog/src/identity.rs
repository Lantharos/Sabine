use image::{ImageFormat, imageops::FilterType};
use winit::{
    icon::{Icon, RgbaIcon},
    window::WindowAttributes,
};

/// Gives a window Sabine's own icon, from the PNG of each size that `png`
/// returns, for windows that speak for Sabine rather than an app.
#[cfg(target_os = "windows")]
pub fn with_sabine_icon(
    attributes: WindowAttributes,
    png: fn(u32) -> &'static [u8],
) -> WindowAttributes {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXICON, SM_CXSMICON, SYSTEM_METRICS_INDEX,
    };
    use winit::platform::windows::WindowAttributesWindows;
    let size = |metric: SYSTEM_METRICS_INDEX| {
        u32::try_from(unsafe { GetSystemMetrics(metric) }).unwrap_or(32)
    };
    attributes
        .with_window_icon(sabine_icon(png, size(SM_CXSMICON)))
        .with_platform_attributes(Box::new(
            WindowAttributesWindows::default().with_taskbar_icon(sabine_icon(png, size(SM_CXICON))),
        ))
}

/// Gives a window Sabine's own icon, from the PNG of each size that `png`
/// returns, for windows that speak for Sabine rather than an app.
#[cfg(not(target_os = "windows"))]
pub fn with_sabine_icon(
    attributes: WindowAttributes,
    png: fn(u32) -> &'static [u8],
) -> WindowAttributes {
    attributes.with_window_icon(sabine_icon(png, 64))
}

fn sabine_icon(png: fn(u32) -> &'static [u8], size: u32) -> Option<Icon> {
    let image = image::load_from_memory_with_format(png(size), ImageFormat::Png)
        .ok()?
        .resize_exact(size, size, FilterType::Lanczos3)
        .into_rgba8();
    RgbaIcon::new(image.into_raw(), size, size)
        .ok()
        .map(Icon::from)
}
