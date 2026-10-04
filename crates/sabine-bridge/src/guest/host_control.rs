use serde::Serialize;
use serde_json::Value;

use super::create::{GuestBounds, GuestCreateOptions};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GuestDownloadAction {
    Accept,
    Cancel,
    Pause,
    Resume,
}

/// Guest operations the app sends to a window's browser host. They take
/// effect without an answer; pages observe the results through guest events.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum GuestHostControl {
    Create(GuestCreateOptions),
    Destroy {
        id: String,
    },
    Navigate {
        id: String,
        url: String,
    },
    SetBounds {
        id: String,
        bounds: GuestBounds,
    },
    SetVisible {
        id: String,
        visible: bool,
    },
    /// Hides every guest while the page draws over them, such as a dialog.
    SetCovered {
        covered: bool,
    },
    Focus {
        id: String,
    },
    Reload {
        id: String,
        ignore_cache: bool,
    },
    GoBack {
        id: String,
    },
    GoForward {
        id: String,
    },
    SetZoom {
        id: String,
        factor: f64,
    },
    ExecuteJavaScript {
        id: String,
        code: String,
    },
    DownloadAction {
        download_id: String,
        action: GuestDownloadAction,
        #[serde(skip_serializing_if = "Option::is_none")]
        save_path: Option<String>,
        show_dialog: bool,
    },
}

impl GuestHostControl {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Create(_) => "guest.create",
            Self::Destroy { .. } => "guest.destroy",
            Self::Navigate { .. } => "guest.navigate",
            Self::SetBounds { .. } => "guest.setBounds",
            Self::SetVisible { .. } => "guest.setVisible",
            Self::SetCovered { .. } => "guest.setCovered",
            Self::Focus { .. } => "guest.focus",
            Self::Reload { .. } => "guest.reload",
            Self::GoBack { .. } => "guest.goBack",
            Self::GoForward { .. } => "guest.goForward",
            Self::SetZoom { .. } => "guest.setZoom",
            Self::ExecuteJavaScript { .. } => "guest.executeJavaScript",
            Self::DownloadAction { .. } => "guest.downloadAction",
        }
    }

    pub fn to_host_value(&self) -> Value {
        serde_json::to_value(self).expect("guest controls serialize to JSON")
    }
}
