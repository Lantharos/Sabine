use sabine_service::manifest::MenuEntry;

use super::{MenuItem, MenuRole};

/// The app menu that `[[menu]]` in `Sabine.toml` describes.
pub(crate) fn from_manifest(entries: Vec<MenuEntry>) -> Result<Vec<MenuItem>, String> {
    let items = entries
        .into_iter()
        .map(item_from_manifest)
        .collect::<Result<Vec<_>, _>>()?;
    super::check(&items)?;
    Ok(items)
}

fn item_from_manifest(entry: MenuEntry) -> Result<MenuItem, String> {
    let role = entry
        .role
        .map(|role| {
            serde_json::from_value::<MenuRole>(serde_json::Value::String(role.clone()))
                .map_err(|_| format!("{role} is not a menu role"))
        })
        .transpose()?;
    Ok(MenuItem {
        id: entry.id,
        label: entry.label,
        role,
        accelerator: entry.accelerator,
        enabled: entry.enabled.unwrap_or(true),
        checked: entry.checked,
        separator: entry.separator,
        items: from_manifest(entry.items)?,
    })
}

#[cfg(test)]
mod tests {
    use sabine_service::manifest::SabineFile;

    use super::*;

    #[test]
    fn manifests_describe_the_app_menu_bar() {
        let (file, unknown) = SabineFile::parse(
            r#"
            [[menu]]
            label = "File"
            items = [
              { id = "new", label = "New Note", accelerator = "Primary+N" },
              { separator = true },
              { role = "quit" },
            ]
            "#,
        )
        .unwrap();
        assert!(unknown.is_empty());
        assert_eq!(
            from_manifest(file.menu).unwrap(),
            vec![MenuItem::submenu(
                "File",
                vec![
                    MenuItem::new("new", "New Note").accelerator("Primary+N"),
                    MenuItem::separator(),
                    MenuItem::role(MenuRole::Quit),
                ],
            )]
        );
    }

    #[test]
    fn unknown_roles_are_refused() {
        let (file, _) =
            SabineFile::parse("[[menu]]\nlabel = \"File\"\nitems = [{ role = \"explode\" }]")
                .unwrap();
        assert!(from_manifest(file.menu).is_err());
    }
}
