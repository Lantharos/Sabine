use winit::window::WindowAttributes;

/// Setup and recovery windows speak for Sabine, so they carry its icon.
pub(super) fn sabine(attributes: WindowAttributes) -> WindowAttributes {
    sabine_dialog::with_sabine_icon(attributes, sabine_service::icon_png)
}

/// An update prompt speaks for the app, so it carries the app's own icon.
#[cfg(target_os = "windows")]
pub(super) fn app(attributes: WindowAttributes) -> WindowAttributes {
    crate::desktop::with_app_icon(
        attributes,
        winit::platform::windows::WindowAttributesWindows::default(),
    )
}

/// Other desktops find the app's icon from the app itself.
#[cfg(not(target_os = "windows"))]
pub(super) fn app(attributes: WindowAttributes) -> WindowAttributes {
    attributes
}
