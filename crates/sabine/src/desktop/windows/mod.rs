mod app_icon;
mod jump_list;
mod registry;
mod ui_thread;

pub(super) use super::hotkeys::GlobalShortcuts;
pub(crate) use app_icon::with_app_icon;
pub(super) use jump_list::register_app_actions;
pub(super) use registry::{
    register_deep_links, register_native_messaging_host, write_autostart_entry,
};
pub(super) use ui_thread::{UiQueue, UiThread};

/// Passes the foreground, which the launch or click that reached the app
/// lent it, on to one of its window hosts, so the window it focuses may come
/// forward: Windows lets only the process the person just used do that.
pub(crate) fn allow_foreground(process: u32) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(process);
    }
}
