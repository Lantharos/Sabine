use sabine_bridge::BridgeCommand;
use sabine_platform::{TrayMenuItem, TrayMenuItemKind};
use serde::Deserialize;

use super::TrayHandle;
use super::icon::IconSource;

pub(crate) const UPDATE_COMMAND: &str = "sabine.tray.update";

/// A page's change to the tray icon. Fields left out stay as they are. The
/// icon is one of the app's resources, named in its manifest, or the image
/// the call carries as its bytes.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Update {
    #[serde(default, with = "serde_with_null")]
    icon: Option<Option<String>>,
    #[serde(default)]
    template: bool,
    #[serde(default, with = "serde_with_null")]
    tooltip: Option<Option<String>>,
    menu: Option<Vec<MenuItem>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MenuItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    label: String,
    action: Option<String>,
    #[serde(default = "enabled")]
    enabled: bool,
    #[serde(default, rename = "type")]
    kind: MenuItemType,
    #[serde(default)]
    checked: bool,
    #[serde(default)]
    items: Vec<MenuItem>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MenuItemType {
    #[default]
    Normal,
    Separator,
    Checkbox,
    Submenu,
}

fn enabled() -> bool {
    true
}

impl From<MenuItem> for TrayMenuItem {
    fn from(item: MenuItem) -> Self {
        let kind = match item.kind {
            MenuItemType::Normal => TrayMenuItemKind::Normal,
            MenuItemType::Separator => TrayMenuItemKind::Separator,
            MenuItemType::Checkbox => TrayMenuItemKind::Checkbox {
                checked: item.checked,
            },
            MenuItemType::Submenu => {
                TrayMenuItemKind::Submenu(item.items.into_iter().map(Into::into).collect())
            }
        };
        Self {
            id: item.id,
            label: item.label,
            action: item.action,
            enabled: item.enabled,
            kind,
        }
    }
}

fn resource(name: String) -> Result<IconSource, String> {
    crate::resource_path(&name)
        .map(IconSource::File)
        .ok_or_else(|| format!("{name} is not one of the app's resources"))
}

/// Distinguishes a field set to `null` from one left out.
mod serde_with_null {
    use serde::{Deserialize, Deserializer};

    pub(super) fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Some)
    }
}

impl TrayHandle {
    pub(crate) fn update_from_page(&self, command: BridgeCommand) -> Result<(), String> {
        let update =
            serde_json::from_value::<Update>(command.params).map_err(|error| error.to_string())?;
        let icon = match (update.icon, command.body) {
            (Some(_), Some(_)) => {
                return Err("the tray icon is either a resource or bytes, not both".to_string());
            }
            (Some(name), None) => Some(name.map(resource).transpose()?),
            (None, Some(bytes)) => Some(Some(IconSource::Bytes(bytes))),
            (None, None) => None,
        };
        if let Some(icon) = icon {
            self.show_icon(icon, update.template);
        }
        if let Some(tooltip) = update.tooltip {
            self.set_tooltip(tooltip);
        }
        if let Some(menu) = update.menu {
            self.set_menu(menu.into_iter().map(Into::into).collect());
        }
        Ok(())
    }
}
