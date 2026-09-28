#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod x11;

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
    backend: LinuxBackend,
    #[cfg(target_os = "windows")]
    _hook: windows::WindowsInhibitor,
}

#[cfg(target_os = "linux")]
enum LinuxBackend {
    Wayland(#[expect(dead_code, reason = "released on drop")] wayland::WaylandInhibitor),
    X11(Box<x11::X11Inhibitor>),
}

impl ShortcutInhibitor {
    #[cfg(target_os = "linux")]
    pub fn new(window: &dyn Window, focused: bool) -> Result<Self, String> {
        let display = window
            .display_handle()
            .map_err(|error| error.to_string())?
            .as_raw();
        let surface = window
            .window_handle()
            .map_err(|error| error.to_string())?
            .as_raw();
        let backend = match (display, surface) {
            (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) => {
                LinuxBackend::Wayland(unsafe {
                    wayland::WaylandInhibitor::new(
                        display.display.as_ptr(),
                        surface.surface.as_ptr(),
                    )
                }?)
            }
            (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(surface)) => {
                let display = display
                    .display
                    .ok_or("The X11 display connection is unavailable")?;
                LinuxBackend::X11(Box::new(x11::X11Inhibitor::new(
                    display.as_ptr().cast(),
                    surface.window,
                    focused,
                )?))
            }
            _ => return Err("This window system cannot inhibit shortcuts".to_string()),
        };
        Ok(Self { backend })
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

    /// X11 only holds its keyboard grab while the window is focused; Wayland
    /// compositors follow focus on their own.
    #[cfg(target_os = "linux")]
    pub fn set_focused(&mut self, focused: bool) {
        if let LinuxBackend::X11(inhibitor) = &mut self.backend {
            inhibitor.set_focused(focused);
        }
    }
}
