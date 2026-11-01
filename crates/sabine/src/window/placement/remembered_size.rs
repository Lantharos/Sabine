//! The size and maximized state a window closed with, kept for the next
//! launch in the app's state folder.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::config::SabineWindowConfig;

#[derive(Deserialize, Serialize)]
struct RememberedSize {
    width: u32,
    height: u32,
    maximized: bool,
}

/// `window.json` in `$XDG_STATE_HOME/<app id>` on Linux and in the local app
/// data folder elsewhere.
fn path(app_id: &str) -> Option<PathBuf> {
    Some(
        dirs::state_dir()
            .or_else(dirs::data_local_dir)?
            .join(app_id)
            .join("window.json"),
    )
}

/// Opens the window as it last closed, never smaller than its minimum size.
pub(crate) fn restore(config: &mut SabineWindowConfig) {
    let Some(remembered) = config
        .app_id
        .as_deref()
        .and_then(path)
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<RememberedSize>(&bytes).ok())
    else {
        return;
    };
    config.width = remembered.width.max(config.min_width);
    config.height = remembered.height.max(config.min_height);
    config.maximized = remembered.maximized;
}

pub(crate) fn save(app_id: &str, width: u32, height: u32, maximized: bool) {
    let Some(path) = path(app_id) else {
        return;
    };
    let remembered = RememberedSize {
        width,
        height,
        maximized,
    };
    if let Some(folder) = path.parent()
        && std::fs::create_dir_all(folder).is_ok()
        && let Ok(json) = serde_json::to_vec(&remembered)
        && let Err(error) = std::fs::write(&path, json)
    {
        sabine_runtime::report_error(
            "window",
            format!("could not remember the window size: {error}"),
        );
    }
}
