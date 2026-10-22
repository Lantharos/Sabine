use image::{ImageFormat, imageops::FilterType};
use winit::{
    icon::{Icon, RgbaIcon},
    window::WindowAttributes,
};

/// Gives a setup or recovery window Sabine's own icon, since it speaks for
/// Sabine rather than the app that started it.
#[cfg(target_os = "windows")]
pub(super) fn with_sabine_icon(attributes: WindowAttributes) -> WindowAttributes {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXICON, SM_CXSMICON, SYSTEM_METRICS_INDEX,
    };
    use winit::platform::windows::WindowAttributesWindows;
    let size = |metric: SYSTEM_METRICS_INDEX| {
        u32::try_from(unsafe { GetSystemMetrics(metric) }).unwrap_or(32)
    };
    attributes
        .with_window_icon(sabine_icon(size(SM_CXSMICON)))
        .with_platform_attributes(Box::new(
            WindowAttributesWindows::default().with_taskbar_icon(sabine_icon(size(SM_CXICON))),
        ))
}

/// Gives a setup or recovery window Sabine's own icon, since it speaks for
/// Sabine rather than the app that started it.
#[cfg(not(target_os = "windows"))]
pub(super) fn with_sabine_icon(attributes: WindowAttributes) -> WindowAttributes {
    attributes.with_window_icon(sabine_icon(64))
}

fn sabine_icon(size: u32) -> Option<Icon> {
    let image =
        image::load_from_memory_with_format(sabine_service::icon_png(size), ImageFormat::Png)
            .ok()?
            .resize_exact(size, size, FilterType::Lanczos3)
            .into_rgba8();
    RgbaIcon::new(image.into_raw(), size, size)
        .ok()
        .map(Icon::from)
}
