#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

#[cfg(target_os = "windows")]
pub use windows::SystemKey;

/// Keeps the desktop's own keyboard shortcuts away from a window while it has
/// focus, so the page receives every key. Dropping it gives them back.
pub struct ShortcutInhibitor {
    #[cfg(target_os = "linux")]
    _wayland: wayland::WaylandInhibitor,
    #[cfg(target_os = "windows")]
    _hook: windows::WindowsInhibitor,
}

impl ShortcutInhibitor {
    #[cfg(target_os = "linux")]
    pub fn new(window: &dyn Window) -> Result<Self, String> {
        let RawDisplayHandle::Wayland(display) = window
            .display_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("The window has no Wayland display".to_string());
        };
        let RawWindowHandle::Wayland(surface) = window
            .window_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("The window has no Wayland surface".to_string());
        };
        let wayland = unsafe {
            wayland::WaylandInhibitor::new(display.display.as_ptr(), surface.surface.as_ptr())
        }?;
        Ok(Self { _wayland: wayland })
    }

    /// Install the hook. Ctrl, Alt and the Windows keys stop reaching the OS
    /// while the window is in the foreground and arrive through `on_key`.
    #[cfg(target_os = "windows")]
    pub fn new(window: &dyn Window, on_key: impl Fn(SystemKey) + 'static) -> Result<Self, String> {
        let RawWindowHandle::Win32(handle) = window
            .window_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("The window has no Win32 handle".to_string());
        };
        let window = ::windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut _);
        Ok(Self {
            _hook: windows::WindowsInhibitor::new(window, on_key)?,
        })
    }
}
