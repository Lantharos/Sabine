#[cfg(target_os = "linux")]
use std::path::Path;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod desktop_wait;
mod process;
mod process_tree;

pub use process::{SabineProcess, SabineProcessHandle, WindowId};
pub(crate) use process_tree::{
    ManagedChild, prepare_child_command, prepare_detachable_child_command,
};
#[cfg(debug_assertions)]
pub(crate) use sabine_host::ensure_host;

#[cfg(not(debug_assertions))]
pub(crate) fn ensure_host(runtime_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
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
