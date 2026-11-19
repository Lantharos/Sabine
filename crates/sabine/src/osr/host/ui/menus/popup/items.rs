use std::sync::Arc;

use crate::MenuItem;

/// An entry of a menu a page shows. An entry with items opens them as a
/// submenu.
#[derive(Clone, Debug)]
pub(super) struct PanelItem {
    /// The id the entry reports when chosen.
    pub(super) id: Arc<str>,
    pub(super) label: String,
    pub(super) enabled: bool,
    pub(super) separator: bool,
    /// Whether the entry turns on and off, with a check mark while on.
    pub(super) checkable: bool,
    pub(super) checked: bool,
    pub(super) shortcut: Option<Arc<str>>,
    pub(super) items: Vec<PanelItem>,
}

/// The menu's items as the window draws them, without the roles this
/// desktop lacks. `app` names the app in labels such as "Quit Notes".
pub(super) fn panel_items(items: &[MenuItem], app: &str) -> Vec<PanelItem> {
    items
        .iter()
        .filter(|item| item.role.is_none_or(crate::MenuRole::available))
        .map(|item| PanelItem {
            id: item.chosen_id().into(),
            label: item.shown_label(app),
            enabled: item.enabled,
            separator: item.separator,
            checkable: item.checked.is_some(),
            checked: item.checked == Some(true),
            shortcut: item
                .shown_accelerator()
                .map(|accelerator| accelerator.display().into()),
            items: panel_items(&item.items, app),
        })
        .collect()
}
