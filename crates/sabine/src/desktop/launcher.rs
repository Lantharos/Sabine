//! What the app's Dock or taskbar icon shows besides the icon itself: a
//! badge, a progress bar, and the files the app opened recently. On Windows
//! every window's taskbar button shows the badge and progress, so they go to
//! the windows.

use std::path::Path;
use std::sync::Arc;

use sabine_bridge::{BridgeCommand, BridgeError, BridgeResponse};
use sabine_platform::{AppBadge, AppProgress, RecentDocuments};
use serde::{Deserialize, Serialize};

use crate::{SabineProcessHandle, SabineWindow};

#[cfg(target_os = "linux")]
use super::platform::LauncherEntry;

#[derive(Clone)]
pub(crate) struct Launcher {
    #[cfg(target_os = "linux")]
    entry: LauncherEntry,
    #[cfg(target_os = "windows")]
    windows: crate::BridgeEventEmitter,
    recent: Arc<RecentDocuments>,
}

impl Launcher {
    /// The list of recent files names the app by its title.
    #[cfg(target_os = "linux")]
    pub(crate) fn new(
        config: &crate::window::config::SabineWindowConfig,
    ) -> crate::SabineResult<Self> {
        let program = crate::launch::executable::launch_executable().map_err(|error| {
            crate::SabineError::CreationFailed {
                message: format!("could not find the app's executable: {error}"),
            }
        })?;
        Ok(Self {
            entry: LauncherEntry::new(crate::osr::launch::require_app_id(config)?),
            recent: Arc::new(RecentDocuments::new(&config.title, &program)),
        })
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn new(windows: crate::BridgeEventEmitter) -> Self {
        Self {
            windows,
            recent: Arc::default(),
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn new() -> Self {
        Self {
            recent: Arc::default(),
        }
    }

    pub(crate) fn set_badge(&self, badge: Option<AppBadge>) {
        #[cfg(target_os = "linux")]
        self.entry.set_badge(badge);
        #[cfg(target_os = "macos")]
        super::platform::set_badge(badge);
        #[cfg(target_os = "windows")]
        self.windows.set_badge(badge.as_ref());
    }

    pub(crate) fn set_progress(&self, progress: AppProgress) {
        #[cfg(target_os = "linux")]
        self.entry.set_progress(progress);
        #[cfg(target_os = "macos")]
        super::platform::set_progress(progress);
        #[cfg(target_os = "windows")]
        self.windows.set_progress(progress);
    }

    pub(crate) fn add_recent_document(&self, path: &Path) -> Result<(), String> {
        self.recent.add(path)
    }

    pub(crate) fn clear_recent_documents(&self) -> Result<(), String> {
        self.recent.clear()
    }
}

/// The `app.*` commands that change what the app's icon shows.
pub(crate) fn launcher_commands(
    window: SabineWindow,
    handle: &SabineProcessHandle,
) -> SabineWindow {
    let (badge, progress, recent, clear) = (
        handle.clone(),
        handle.clone(),
        handle.clone(),
        handle.clone(),
    );
    window
        .bridge_handler("app.setBadge", move |command| {
            answered(badge_from_json(param(&command, "badge")).map(|shown| badge.set_badge(shown)))
        })
        .bridge_handler("app.setProgress", move |command| {
            answered(
                progress_from_json(param(&command, "progress"))
                    .map(|shown| progress.set_progress(shown)),
            )
        })
        .bridge_handler("app.addRecentDocument", move |command| {
            let path = command
                .params
                .get("path")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| BridgeError::new("app.addRecentDocument requires a `path`"))?;
            answered(recent.add_recent_document(path))
        })
        .bridge_handler("app.clearRecentDocuments", move |_| {
            answered(clear.clear_recent_documents())
        })
}

fn param(command: &BridgeCommand, name: &str) -> serde_json::Value {
    command
        .params
        .get(name)
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

fn answered(result: Result<(), String>) -> sabine_bridge::BridgeResult {
    result
        .map(|()| BridgeResponse::json(serde_json::Value::Null))
        .map_err(BridgeError::new)
}

/// A badge as pages and the app's windows pass it: a count, a few
/// characters, or `null` for none.
#[derive(Deserialize)]
#[serde(untagged)]
enum Badge {
    Count(u64),
    Text(String),
}

fn badge_from_json(value: serde_json::Value) -> Result<Option<AppBadge>, String> {
    let badge = serde_json::from_value::<Option<Badge>>(value)
        .map_err(|_| "a badge is a whole number of zero or more, a text, or null".to_string())?;
    Ok(match badge {
        None => None,
        Some(Badge::Count(count)) => Some(AppBadge::Count(count)),
        #[cfg(not(target_os = "linux"))]
        Some(Badge::Text(text)) => Some(AppBadge::Text(text)),
        #[cfg(target_os = "linux")]
        Some(Badge::Text(text)) => {
            return Err(format!(
                "Linux docks show only counts as badges, not {text:?}"
            ));
        }
    })
}

/// Progress as pages and the app's windows pass it.
#[derive(Deserialize, Serialize)]
#[serde(tag = "state", rename_all = "lowercase", deny_unknown_fields)]
enum Progress {
    Hidden,
    Indeterminate,
    Normal { value: f64 },
    Paused { value: f64 },
    Error { value: f64 },
}

pub(crate) fn progress_from_json(value: serde_json::Value) -> Result<AppProgress, String> {
    Ok(
        match serde_json::from_value(value).map_err(|error| error.to_string())? {
            Progress::Hidden => AppProgress::Hidden,
            #[cfg(not(target_os = "linux"))]
            Progress::Indeterminate => AppProgress::Indeterminate,
            #[cfg(target_os = "linux")]
            Progress::Indeterminate => {
                return Err("Linux docks show no indeterminate progress".to_string());
            }
            Progress::Normal { value } => AppProgress::Normal(value),
            Progress::Paused { value } => AppProgress::Paused(value),
            Progress::Error { value } => AppProgress::Error(value),
        },
    )
}

#[cfg(target_os = "windows")]
pub(crate) fn progress_to_json(progress: AppProgress) -> serde_json::Value {
    let progress = match progress {
        AppProgress::Hidden => Progress::Hidden,
        AppProgress::Indeterminate => Progress::Indeterminate,
        AppProgress::Normal(value) => Progress::Normal { value },
        AppProgress::Paused(value) => Progress::Paused { value },
        AppProgress::Error(value) => Progress::Error { value },
    };
    serde_json::to_value(progress).expect("progress serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_what_pages_pass() {
        assert_eq!(badge_from_json(json!(3)), Ok(Some(AppBadge::Count(3))));
        assert_eq!(badge_from_json(json!(null)), Ok(None));
        assert!(badge_from_json(json!(-1)).is_err());
        assert_eq!(
            progress_from_json(json!({ "state": "paused", "value": 0.5 })),
            Ok(AppProgress::Paused(0.5))
        );
        assert!(progress_from_json(json!({ "state": "normal" })).is_err());
    }
}
