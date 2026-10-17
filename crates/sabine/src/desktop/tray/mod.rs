mod bridge;
mod icon;
#[cfg(not(target_os = "linux"))]
#[path = "native/mod.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "status_notifier/mod.rs"]
mod platform;

pub(crate) use bridge::UPDATE_COMMAND;
pub use platform::TrayHandle;
