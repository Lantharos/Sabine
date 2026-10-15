// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Bundling writes the app's icon into its executable as MAINICON. Windows
// gives a window no icon of its own, so the title bar, taskbar and Alt+Tab
// show it only once the window is handed one at the sizes the system uses.

use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXICON, SM_CXSMICON, SYSTEM_METRICS_INDEX,
};
use winit::{
    dpi::PhysicalSize,
    icon::Icon,
    platform::windows::{WinIcon, WindowAttributesWindows},
    window::WindowAttributes,
};

const RESOURCE: &str = "MAINICON";

/// Gives a window the app's icon, along with its other Windows attributes.
pub(crate) fn with_app_icon(
    attributes: WindowAttributes,
    windows: WindowAttributesWindows,
) -> WindowAttributes {
    attributes
        .with_window_icon(load(SM_CXSMICON))
        .with_platform_attributes(Box::new(windows.with_taskbar_icon(load(SM_CXICON))))
}

fn load(metric: SYSTEM_METRICS_INDEX) -> Option<Icon> {
    let size = u32::try_from(unsafe { GetSystemMetrics(metric) }).ok()?;
    WinIcon::from_resource_name(RESOURCE, Some(PhysicalSize::new(size, size)))
        .ok()
        .map(Icon::from)
}
