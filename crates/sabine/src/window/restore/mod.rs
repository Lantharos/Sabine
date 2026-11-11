//! What each window showed, kept for the desktop to open the window again
//! with it after the session ends while the window is open.
//!
//! Every window has a key, which is also its `xdg_toplevel_tag_v1` tag. While
//! the window is open its page keeps its state in `<key>.json` in the
//! `windows` folder of the app's state folder, and the window host holds
//! `<key>.lock`. Closing the window, or quitting the app, removes both, so only
//! windows still open when the session ended leave state behind. A desktop
//! that reopens the app names the keys of the windows it had in
//! `SABINE_RESTORE_WINDOWS`, and the first window takes up the first of them
//! that has state.

mod files;
mod writer;

use std::path::{Path, PathBuf};

pub(crate) use files::WindowMemory;

pub(crate) const RESTORE_WINDOWS: &str = "SABINE_RESTORE_WINDOWS";
const KEY_BYTES: usize = 16;

pub(crate) fn folder(app_id: &str) -> Option<PathBuf> {
    Some(dirs::state_dir()?.join(app_id).join("windows"))
}

pub(crate) fn fresh_key() -> String {
    sabine_runtime::random_hex(KEY_BYTES).expect("the system hands out random bytes")
}

fn valid(key: &str) -> bool {
    key.len() == KEY_BYTES * 2 && key.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn state_path(folder: &Path, key: &str) -> PathBuf {
    folder.join(format!("{key}.json"))
}

/// The keys of the windows the desktop asked the app to open again.
fn requested() -> Vec<String> {
    std::env::var(RESTORE_WINDOWS)
        .unwrap_or_default()
        .split_whitespace()
        .filter(|key| valid(key))
        .map(str::to_owned)
        .collect()
}

/// The key the app's first window opens with: the first window the desktop
/// asked for that left state behind. What windows that are no longer open
/// left behind is removed away from the launch, apart from the windows the
/// desktop asked for.
pub(crate) fn launched(app_id: &str) -> Option<String> {
    let folder = folder(app_id)?;
    let requested = requested();
    let first = requested
        .iter()
        .find(|key| state_path(&folder, key).is_file())
        .cloned();
    std::thread::spawn(move || files::forget_unclaimed(&folder, &requested));
    first
}
