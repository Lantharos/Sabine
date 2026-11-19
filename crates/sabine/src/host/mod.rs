#[cfg(target_os = "linux")]
use std::path::Path;

mod desktop_events;
mod handle;
#[cfg(target_os = "linux")]
mod installation;
mod page_window;
mod process;
mod process_tree;

pub use handle::SabineProcessHandle;
#[cfg(target_os = "linux")]
pub(crate) use installation::Installation;
pub(crate) use process::{PrimaryWindow, ProcessCommand};
pub use process::{SabineProcess, WindowId};
pub(crate) use process_tree::{ManagedChild, prepare_detachable_child_command, spawn_child};

/// The host the app's windows run in: the one a standalone app ships, or
/// the one this computer's Sabine provides.
pub(crate) fn app_host(
    config: &crate::window::config::SabineWindowConfig,
    runtime_dir: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    if config.standalone() {
        return sabine_host::bundled_host().ok_or_else(|| {
            "The Sabine host this app ships with is missing. Reinstall the app to repair it."
                .to_string()
        });
    }
    ensure_host(runtime_dir)
}

#[cfg(debug_assertions)]
use sabine_host::ensure_host;

#[cfg(not(debug_assertions))]
fn ensure_host(runtime_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    sabine_host::available_host(runtime_dir).ok_or_else(||
        "The shared Sabine host is missing. Repair or update the Sabine installation before launching this app.".to_string())
}

pub(crate) use sabine_service::browser_profile_path as browser_profile_dir;

#[cfg(target_os = "linux")]
pub(crate) fn ld_library_path(release_dir: &Path) -> String {
    let existing = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
    if existing.is_empty() {
        release_dir.display().to_string()
    } else {
        format!("{}:{existing}", release_dir.display())
    }
}
