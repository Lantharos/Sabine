use std::sync::Arc;

use crate::MenuItem;
use crate::osr::protocol::{ContextMenuItem, MenuMark};

/// What choosing an entry of a menu the window draws runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::osr::host) enum Command {
    /// One of Chromium's commands, by its id.
    Chromium(i32),
    /// One of the app's items, by the id it reports.
    App(Arc<str>),
}

/// An entry of a menu the window draws, from Chromium or the app. An entry
/// with items opens them as a submenu.
#[derive(Clone, Debug)]
pub(super) struct PanelItem {
    pub(super) command: Command,
    pub(super) label: String,
    pub(super) enabled: bool,
    pub(super) separator: bool,
    pub(super) mark: MenuMark,
    pub(super) checked: bool,
    pub(super) shortcut: Option<Arc<str>>,
    pub(super) items: Vec<PanelItem>,
}

impl PanelItem {
    pub(super) fn from_chromium(item: ContextMenuItem) -> Self {
        Self {
            command: Command::Chromium(item.id),
            shortcut: shortcut_for(item.id).map(Arc::from),
            label: item.label,
            enabled: item.enabled,
            separator: item.separator,
            mark: item.mark,
            checked: item.checked,
            items: item.items.into_iter().map(Self::from_chromium).collect(),
        }
    }

    pub(super) fn separator() -> Self {
        Self {
            command: Command::Chromium(-1),
            label: String::new(),
            enabled: false,
            separator: true,
            mark: MenuMark::None,
            checked: false,
            shortcut: None,
            items: Vec::new(),
        }
    }
}

/// The app's items as the window draws them, without the roles this
/// desktop lacks. `app` names the app in labels such as "Quit Notes".
pub(super) fn app_items(items: &[MenuItem], app: &str) -> Vec<PanelItem> {
    items
        .iter()
        .filter(|item| item.role.is_none_or(crate::MenuRole::available))
        .map(|item| PanelItem {
            command: Command::App(item.chosen_id().into()),
            label: item.shown_label(app),
            enabled: item.enabled,
            separator: item.separator,
            mark: if item.checked.is_some() {
                MenuMark::Check
            } else {
                MenuMark::None
            },
            checked: item.checked == Some(true),
            shortcut: item
                .shown_accelerator()
                .map(|accelerator| accelerator.display().into()),
            items: app_items(&item.items, app),
        })
        .collect()
}

/// The keyboard shortcut shown beside a CEF edit command, in the desktop's
/// own notation.
fn shortcut_for(command: i32) -> Option<&'static str> {
    let shortcuts: [&str; 6] = if cfg!(target_os = "macos") {
        ["⌘Z", "⇧⌘Z", "⌘X", "⌘C", "⌘V", "⌘A"]
    } else if cfg!(windows) {
        ["Ctrl+Z", "Ctrl+Y", "Ctrl+X", "Ctrl+C", "Ctrl+V", "Ctrl+A"]
    } else {
        [
            "Ctrl+Z",
            "Ctrl+Shift+Z",
            "Ctrl+X",
            "Ctrl+C",
            "Ctrl+V",
            "Ctrl+A",
        ]
    };
    match command {
        110 => Some(shortcuts[0]),
        111 => Some(shortcuts[1]),
        112 => Some(shortcuts[2]),
        113 => Some(shortcuts[3]),
        114 => Some(shortcuts[4]),
        117 => Some(shortcuts[5]),
        _ => None,
    }
}
