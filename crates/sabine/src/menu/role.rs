use serde::{Deserialize, Serialize};

use super::Accelerator;

/// What a menu item does on its own, the way desktop apps' standard items do.
///
/// `About`, `Hide`, `HideOthers`, `ShowAll` and `Services` belong to the
/// macOS app menu and are left out elsewhere, except `About`, which reports
/// its id there so the app can show its own. `Window` marks the submenu
/// macOS lists the window in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuRole {
    About,
    Hide,
    HideOthers,
    ShowAll,
    Services,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Minimize,
    Zoom,
    Fullscreen,
    Close,
    Window,
}

impl MenuRole {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::About => "about",
            Self::Hide => "hide",
            Self::HideOthers => "hideOthers",
            Self::ShowAll => "showAll",
            Self::Services => "services",
            Self::Quit => "quit",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Cut => "cut",
            Self::Copy => "copy",
            Self::Paste => "paste",
            Self::SelectAll => "selectAll",
            Self::Minimize => "minimize",
            Self::Zoom => "zoom",
            Self::Fullscreen => "fullscreen",
            Self::Close => "close",
            Self::Window => "window",
        }
    }

    pub(crate) fn label(self, app: &str) -> String {
        match self {
            Self::About => format!("About {app}"),
            Self::Hide => format!("Hide {app}"),
            Self::HideOthers => "Hide Others".into(),
            Self::ShowAll => "Show All".into(),
            Self::Services => "Services".into(),
            Self::Quit if cfg!(windows) => "Exit".into(),
            Self::Quit if cfg!(target_os = "macos") => format!("Quit {app}"),
            Self::Quit => "Quit".into(),
            Self::Undo => "Undo".into(),
            Self::Redo => "Redo".into(),
            Self::Cut => "Cut".into(),
            Self::Copy => "Copy".into(),
            Self::Paste => "Paste".into(),
            Self::SelectAll => "Select All".into(),
            Self::Minimize => "Minimize".into(),
            Self::Zoom => "Zoom".into(),
            Self::Fullscreen if cfg!(target_os = "macos") => "Enter Full Screen".into(),
            Self::Fullscreen => "Full Screen".into(),
            Self::Close => "Close Window".into(),
            Self::Window => "Window".into(),
        }
    }

    pub(crate) fn accelerator(self) -> Option<Accelerator> {
        let text = match self {
            Self::Hide if cfg!(target_os = "macos") => "Primary+H",
            Self::HideOthers if cfg!(target_os = "macos") => "Primary+Alt+H",
            Self::Quit if !cfg!(windows) => "Primary+Q",
            Self::Undo => "Primary+Z",
            Self::Redo if cfg!(windows) => "Primary+Y",
            Self::Redo => "Primary+Shift+Z",
            Self::Cut => "Primary+X",
            Self::Copy => "Primary+C",
            Self::Paste => "Primary+V",
            Self::SelectAll => "Primary+A",
            Self::Minimize if cfg!(target_os = "macos") => "Primary+M",
            Self::Fullscreen if cfg!(target_os = "macos") => "Control+Primary+F",
            Self::Fullscreen => "F11",
            Self::Close => "Primary+W",
            _ => return None,
        };
        Accelerator::parse(text).ok()
    }

    /// Whether the role edits the page's text, which the page does itself.
    pub(crate) fn edits(self) -> bool {
        matches!(
            self,
            Self::Undo | Self::Redo | Self::Cut | Self::Copy | Self::Paste | Self::SelectAll
        )
    }

    /// Whether the role means anything on this desktop.
    pub(crate) fn available(self) -> bool {
        cfg!(target_os = "macos")
            || !matches!(
                self,
                Self::Hide | Self::HideOthers | Self::ShowAll | Self::Services
            )
    }
}
