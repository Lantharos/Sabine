mod instance;
mod registry;
mod ui_thread;

pub(super) use super::hotkeys::GlobalShortcuts;
pub(super) use instance::SingleInstanceGuard;
pub(super) use registry::{
    register_deep_links, register_native_messaging_host, write_autostart_entry,
};
pub(super) use ui_thread::{UiQueue, UiThread};
