//! The size and maximized state a window closed with, kept for the next
//! launch in the app's state folder.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::window::config::SabineWindowConfig;

const FILE: &str = "window.json";

#[derive(Deserialize, Serialize)]
struct RememberedSize {
    width: u32,
    height: u32,
    maximized: bool,
}

/// `$XDG_STATE_HOME/<app id>` on Linux and the app's folder in the local app
/// data folder elsewhere.
fn folder(app_id: &str) -> Option<PathBuf> {
    Some(
        dirs::state_dir()
            .or_else(dirs::data_local_dir)?
            .join(app_id),
    )
}

/// Opens the window as it last closed, never smaller than its minimum size.
pub(crate) fn restore(config: &mut SabineWindowConfig) {
    let Some(remembered) = config
        .app_id
        .as_deref()
        .and_then(folder)
        .and_then(|folder| std::fs::read(folder.join(FILE)).ok())
        .and_then(|bytes| serde_json::from_slice::<RememberedSize>(&bytes).ok())
    else {
        return;
    };
    config.width = remembered.width.max(config.min_width);
    config.height = remembered.height.max(config.min_height);
    config.maximized = remembered.maximized;
}

pub(crate) fn save(app_id: &str, width: u32, height: u32, maximized: bool) {
    let Some(folder) = folder(app_id) else {
        return;
    };
    let remembered = RememberedSize {
        width,
        height,
        maximized,
    };
    let json = serde_json::to_vec(&remembered).expect("the window size serializes");
    if let Err(error) =
        std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(folder.join(FILE), json))
    {
        sabine_runtime::report_error(
            "window",
            format!("could not remember the window size: {error}"),
        );
    }
}
