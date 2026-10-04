mod appimage;
mod links;
mod shortcuts;
mod ui_thread;
mod util;

use sabine_platform::{AutostartEntry, DeepLinkRegistration, NativeMessagingHost};

pub(super) use super::unix_instance::SingleInstanceGuard;
pub(super) use appimage::native_host_program;
pub(super) use shortcuts::GlobalShortcuts;
pub(super) use ui_thread::{UiQueue, UiThread};

pub(super) fn write_autostart_entry(entry: &AutostartEntry) -> Result<(), String> {
    links::write_autostart_entry(entry).map_err(|error| error.to_string())
}

pub(super) fn register_deep_links(registration: &DeepLinkRegistration) -> Result<(), String> {
    links::register_deep_links(registration).map_err(|error| error.to_string())
}

pub(super) fn register_native_messaging_host(host: &NativeMessagingHost) -> Result<(), String> {
    super::native_messaging::write_manifests(host)
        .map(drop)
        .map_err(|error| error.to_string())
}
