//! The menus an app fills: menus its pages show at a point, and the app menu
//! bar.

mod accelerator;
mod manifest;
mod role;

pub(crate) use accelerator::Accelerator;
pub(crate) use manifest::from_manifest;
pub use role::MenuRole;

use serde::{Deserialize, Serialize};

/// One item of a menu: a command, a separator, or a submenu of more items.
///
/// Items are chosen by `id`, which [`SabineWindow::on_menu_item`](crate::SabineWindow::on_menu_item)
/// and the pages' `menu.select` event report. An item with a [`MenuRole`]
/// does what its role says instead, and takes the role's label and
/// accelerator unless it has its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<MenuRole>,
    /// The keys that choose the item while the page leaves them unhandled,
    /// such as `Primary+Shift+S`, where `Primary` is Command on macOS and
    /// Control elsewhere. The menu shows them beside the label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accelerator: Option<String>,
    pub enabled: bool,
    /// Draws a check mark, on when `true`, for an item that turns on and off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub separator: bool,
    /// The items of the submenu this item opens.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<MenuItem>,
}

impl Default for MenuItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            role: None,
            accelerator: None,
            enabled: true,
            checked: None,
            separator: false,
            items: Vec::new(),
        }
    }
}

impl MenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn separator() -> Self {
        Self {
            separator: true,
            ..Self::default()
        }
    }

    pub fn submenu(label: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self {
            label: label.into(),
            items,
            ..Self::default()
        }
    }

    /// An item that does what `role` says, such as quitting or copying.
    pub fn role(role: MenuRole) -> Self {
        Self {
            role: Some(role),
            ..Self::default()
        }
    }

    pub fn accelerator(mut self, accelerator: impl Into<String>) -> Self {
        self.accelerator = Some(accelerator.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Makes the item open `items` as a submenu, as a role item that names
    /// a submenu, such as [`MenuRole::Window`], does.
    pub fn items(mut self, items: Vec<MenuItem>) -> Self {
        self.items = items;
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// What choosing the item reports: its id, or its role's name.
    pub(crate) fn chosen_id(&self) -> &str {
        match (&self.role, self.id.is_empty()) {
            (Some(role), true) => role.name(),
            _ => &self.id,
        }
    }

    /// The label the menu shows, the role's own when the item has none.
    /// `app` names the app in labels such as "Quit Notes".
    pub(crate) fn shown_label(&self, app: &str) -> String {
        match &self.role {
            Some(role) if self.label.is_empty() => role.label(app),
            _ => self.label.clone(),
        }
    }

    pub(crate) fn shown_accelerator(&self) -> Option<Accelerator> {
        match &self.accelerator {
            Some(accelerator) => Accelerator::parse(accelerator).ok(),
            None => self.role.and_then(MenuRole::accelerator),
        }
    }
}

/// A menu item someone chose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuActivation {
    pub window: crate::WindowId,
    /// The item's id, or its role's name for a role item without one.
    pub id: String,
    pub menu: MenuKind,
}

/// The menu an item was chosen from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MenuKind {
    /// A menu a page showed with `menu.popup`.
    Popup,
    /// The app menu bar, or a page drawing it.
    App,
}

/// Checks that every accelerator of `items` and their submenus parses.
pub(crate) fn check(items: &[MenuItem]) -> Result<(), String> {
    items.iter().try_for_each(|item| {
        if let Some(accelerator) = &item.accelerator {
            Accelerator::parse(accelerator)?;
        }
        check(&item.items)
    })
}

/// The accelerators that choose an item of `items` when the page leaves the
/// keys unhandled, and the item each chooses. Edit roles are left to the
/// page, which runs them itself.
pub(crate) fn shortcuts(items: &[MenuItem]) -> Vec<(Accelerator, &MenuItem)> {
    let mut shortcuts = Vec::new();
    collect_shortcuts(items, &mut shortcuts);
    shortcuts
}

fn collect_shortcuts<'a>(items: &'a [MenuItem], shortcuts: &mut Vec<(Accelerator, &'a MenuItem)>) {
    for item in items {
        if item.separator || !item.enabled || item.role.is_some_and(|role| !role.available()) {
            continue;
        }
        if item.items.is_empty() {
            if let Some(accelerator) = item.shown_accelerator()
                && !item.role.is_some_and(MenuRole::edits)
            {
                shortcuts.push((accelerator, item));
            }
        } else {
            collect_shortcuts(&item.items, shortcuts);
        }
    }
}

/// `items` as a page that draws them reads them: with the label and
/// accelerator each shows and the id each reports, and without the roles
/// this desktop lacks. `app` names the app in labels such as "Quit Notes".
pub(crate) fn shown(items: &[MenuItem], app: &str) -> Vec<MenuItem> {
    items
        .iter()
        .filter(|item| item.role.is_none_or(MenuRole::available))
        .map(|item| MenuItem {
            id: item.chosen_id().to_string(),
            label: item.shown_label(app),
            accelerator: item.accelerator.clone().or_else(|| {
                item.shown_accelerator()
                    .map(|accelerator| accelerator.canonical())
            }),
            role: item.role,
            enabled: item.enabled,
            checked: item.checked,
            separator: item.separator,
            items: shown(&item.items, app),
        })
        .collect()
}

/// The item of `items` or their submenus that reports `id` when chosen.
pub(crate) fn find<'a>(items: &'a [MenuItem], id: &str) -> Option<&'a MenuItem> {
    items.iter().find_map(|item| {
        if item.items.is_empty() {
            (!item.separator && item.chosen_id() == id).then_some(item)
        } else {
            find(&item.items, id)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_describe_items_in_the_same_shape_rust_builds_them() {
        let items: Vec<MenuItem> = serde_json::from_str(
            r#"[
                { "label": "File", "items": [
                    { "id": "save", "label": "Save", "accelerator": "Primary+S" },
                    { "separator": true },
                    { "role": "quit" }
                ] }
            ]"#,
        )
        .unwrap();
        assert_eq!(
            items,
            vec![MenuItem::submenu(
                "File",
                vec![
                    MenuItem::new("save", "Save").accelerator("Primary+S"),
                    MenuItem::separator(),
                    MenuItem::role(MenuRole::Quit),
                ],
            )]
        );
    }

    #[test]
    fn role_items_report_their_role_unless_they_have_an_id() {
        let items = vec![
            MenuItem::role(MenuRole::Quit),
            MenuItem::role(MenuRole::About).label("About Notes"),
        ];
        assert!(find(&items, "quit").is_some());
        assert_eq!(
            find(&items, "about").map(|item| item.label.as_str()),
            Some("About Notes")
        );
    }

    #[test]
    fn edit_roles_stay_with_the_page() {
        let items = vec![MenuItem::submenu(
            "Edit",
            vec![
                MenuItem::role(MenuRole::Copy),
                MenuItem::new("find", "Find").accelerator("Primary+F"),
                MenuItem::new("off", "Off")
                    .accelerator("Primary+O")
                    .enabled(false),
            ],
        )];
        let ids = shortcuts(&items)
            .into_iter()
            .map(|(_, item)| item.chosen_id().to_string())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["find"]);
    }
}
