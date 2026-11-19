mod updates;

use sabine_runtime::{RuntimeConfig, RuntimeInfo, RuntimeMode, resolve_runtime};

use crate::window::config::SabineWindowConfig;
use crate::{SabineError, SabineResult};

pub(crate) use updates::{APPLY_UPDATE_ARG, apply_update};

/// Finds the Chromium the app ships with, offers an update it downloaded
/// earlier, and keeps looking for newer ones while it runs. Nothing here
/// touches the Sabine installed on the computer.
pub(crate) fn prepare(config: &SabineWindowConfig) -> SabineResult<RuntimeInfo> {
    let runtime = bundled_runtime(config)?;
    updates::offer_pending(config);
    updates::keep_checking(config);
    Ok(runtime)
}

pub(crate) fn bundled_runtime(config: &SabineWindowConfig) -> SabineResult<RuntimeInfo> {
    let missing = || SabineError::CreationFailed {
        message: "The Chromium this app ships with is missing. Reinstall the app to repair it."
            .to_string(),
    };
    let bundled_dir = config.runtime.bundled_dir.clone().ok_or_else(missing)?;
    resolve_runtime(&RuntimeConfig {
        mode: RuntimeMode::Bundled,
        allow_user_install: false,
        allow_bundled: true,
        bundled_dir: Some(bundled_dir),
    })
    .map_err(|_| missing())
}
