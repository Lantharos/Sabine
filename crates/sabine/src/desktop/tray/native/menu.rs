use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use sabine_platform::{TrayActivation, TrayMenuItem, TrayMenuItemKind};
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};

/// What each item of the shown menu reports when it is clicked. Shared with
/// the menu event handler, which may run on another thread.
#[derive(Clone, Default)]
pub(super) struct MenuEntries(Arc<Mutex<HashMap<MenuId, MenuEntry>>>);

struct MenuEntry {
    item_id: String,
    action: Option<String>,
    checked: Option<bool>,
}

impl MenuEntries {
    /// The activation for a clicked item. A checkbox flips its state, as the
    /// menu itself does.
    pub(super) fn activate(&self, tray_id: &str, id: &MenuId) -> Option<TrayActivation> {
        let mut entries = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let entry = entries.get_mut(id)?;
        if let Some(checked) = &mut entry.checked {
            *checked = !*checked;
        }
        Some(TrayActivation::item(
            tray_id,
            entry.item_id.clone(),
            entry.action.clone(),
            entry.checked,
        ))
    }
}

pub(super) fn build(items: &[TrayMenuItem], entries: &MenuEntries) -> Result<Menu, String> {
    let menu = Menu::new();
    let mut shown = HashMap::new();
    for item in items {
        menu.append(menu_item(item, &mut shown)?.as_ref())
            .map_err(|error| error.to_string())?;
    }
    *entries.0.lock().unwrap_or_else(|error| error.into_inner()) = shown;
    Ok(menu)
}

fn menu_item(
    item: &TrayMenuItem,
    shown: &mut HashMap<MenuId, MenuEntry>,
) -> Result<Box<dyn IsMenuItem>, String> {
    let label = item.label.replace('&', "&&");
    let entry = |checked| MenuEntry {
        item_id: item.id.clone(),
        action: item.action.clone(),
        checked,
    };
    Ok(match &item.kind {
        TrayMenuItemKind::Separator => Box::new(PredefinedMenuItem::separator()),
        TrayMenuItemKind::Normal => {
            let menu_item = MenuItem::new(label, item.enabled, None);
            shown.insert(menu_item.id().clone(), entry(None));
            Box::new(menu_item)
        }
        TrayMenuItemKind::Checkbox { checked } => {
            let menu_item = CheckMenuItem::new(label, item.enabled, *checked, None);
            shown.insert(menu_item.id().clone(), entry(Some(*checked)));
            Box::new(menu_item)
        }
        TrayMenuItemKind::Submenu(children) => {
            let submenu = Submenu::new(label, item.enabled);
            for child in children {
                submenu
                    .append(menu_item(child, shown)?.as_ref())
                    .map_err(|error| error.to_string())?;
            }
            Box::new(submenu)
        }
    })
}
