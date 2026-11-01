use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::host_is_complete;

/// The host Sabine was handed or ships beside the app: `SABINE_HOST_PATH`,
/// one next to the executable, and in release builds the installed one.
pub(crate) fn prebuilt_host_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SABINE_HOST_PATH")
        && let Ok(path) = PathBuf::from(path).canonicalize()
        && host_is_complete(&path)
    {
        return Some(path);
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        let path = installed_host_path(directory);
        if host_is_complete(&path) {
            return Some(path);
        }
    }
    if cfg!(debug_assertions) {
        return None;
    }
    installed_host()
}

/// The host of the newest Sabine installed for this user or for every user.
/// Debug builds compile their own from the runtime's CEF SDK and fall back to
/// this one only when they can't.
pub(crate) fn installed_host() -> Option<PathBuf> {
    let user = installed_system_host(&sabine_runtime::sabine_data_dir());
    let machine = installed_system_host(&sabine_runtime::system_dir());
    match (user, machine) {
        (Some(user), Some(machine)) if machine.0 >= user.0 => Some(machine.1),
        (user, machine) => user.or(machine).map(|(_, path)| path),
    }
}

/// The host of the Sabine installation under a data directory, with its
/// version for choosing between the user's and the machine's.
fn installed_system_host(root: &Path) -> Option<(Vec<u64>, PathBuf)> {
    #[derive(Deserialize)]
    struct CurrentSystem {
        active: String,
    }
    let bin = root.join("bin");
    let current =
        serde_json::from_slice::<CurrentSystem>(&std::fs::read(bin.join("current.json")).ok()?)
            .ok()?;
    let path = installed_host_path(&bin.join("versions").join(&current.active));
    let version = current
        .active
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect();
    host_is_complete(&path).then_some((version, path))
}

fn installed_host_path(directory: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        directory.join("sabine-host.app/Contents/MacOS/sabine-host")
    } else {
        directory.join(crate::host_binary_name())
    }
}
