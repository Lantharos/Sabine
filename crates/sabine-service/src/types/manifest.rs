use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use super::{AppUpdatesSection, PermissionsSection, SpellcheckSection};
use crate::SandboxSection;

/// `Sabine.toml`, as both the CLI and the app read it.
#[derive(Debug, Default, Deserialize)]
pub struct SabineFile {
    #[serde(default)]
    pub app: AppSection,
    #[serde(default)]
    pub web: WebSection,
    #[serde(default)]
    pub window: WindowSection,
    pub updates: Option<AppUpdatesSection>,
    pub sandbox: Option<SandboxSection>,
    #[serde(default)]
    pub crash: CrashSection,
    #[serde(default)]
    pub permissions: PermissionsSection,
    #[serde(default)]
    pub spellcheck: SpellcheckSection,
    #[serde(default)]
    pub resources: BTreeMap<String, String>,
    #[serde(default)]
    pub install: InstallSection,
    pub bindings: Option<BindingsSection>,
    #[serde(default)]
    pub menu: Vec<MenuEntry>,
}

impl SabineFile {
    /// Reads a manifest, and names every key in it that Sabine doesn't know,
    /// such as `web.dev_ulr`.
    pub fn parse(text: &str) -> Result<(Self, Vec<String>), toml::de::Error> {
        let mut unknown = Vec::new();
        let file = serde_ignored::deserialize(toml::Deserializer::parse(text)?, |key| {
            unknown.push(key.to_string())
        })?;
        Ok((file, unknown))
    }

    /// The Cargo manifest of the app's executable.
    pub fn cargo_manifest(&self, source_dir: &Path) -> PathBuf {
        source_dir.join(self.app.cargo_manifest.as_deref().unwrap_or("Cargo.toml"))
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct AppSection {
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    /// The Sabine build a bundle was made with, which bundles record.
    pub sabine: Option<String>,
    pub publisher: Option<String>,
    pub maintainer: Option<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub installer_image: Option<String>,
    pub shared_runtime: Option<bool>,
    #[serde(default)]
    pub mime_types: Vec<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Vec<String>>,
    pub generic_name: Option<String>,
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub listed: Option<bool>,
    pub autostart_arguments: Option<String>,
    #[serde(default)]
    pub actions: Vec<AppAction>,
    pub cargo_manifest: Option<String>,
    pub command: Option<String>,
}

/// Something people start the app for from its launcher, Dock or taskbar
/// menu, such as opening a new window. The app starts with its arguments, or
/// a running app gets them as a second launch would.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppAction {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub arguments: Vec<String>,
}

impl AppAction {
    /// Checks every action, and that no two share an id.
    pub fn validate_all(actions: &[Self]) -> Result<(), String> {
        for (index, action) in actions.iter().enumerate() {
            action.validate()?;
            if actions[..index].iter().any(|other| other.id == action.id) {
                return Err(format!("two app actions share the id {}", action.id));
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(format!(
                "app action id {:?} may contain only letters, digits and hyphens",
                self.id
            ));
        }
        if self.name.trim().is_empty()
            || std::iter::once(&self.name)
                .chain(&self.arguments)
                .any(|text| text.chars().any(char::is_control))
        {
            return Err(format!(
                "app action {} needs a name, and its name and arguments may contain no control characters",
                self.id
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct WebSection {
    pub root: Option<String>,
    pub dist: Option<String>,
    pub entry: Option<String>,
    pub build: Option<String>,
    pub url: Option<String>,
    pub dev_url: Option<String>,
    pub dev_port: Option<u16>,
    pub dev_command: Option<String>,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    #[serde(default)]
    pub local_files: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct WindowSection {
    #[serde(default)]
    pub splash: bool,
    pub splash_delay_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CrashSection {
    /// Where Chromium sends the app's crash reports. Without it, reports stay
    /// in the app's data folder.
    pub upload_url: Option<String>,
}

/// One `[[menu]]` of the app menu bar, or an item of one.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct MenuEntry {
    pub id: String,
    pub label: String,
    pub role: Option<String>,
    pub accelerator: Option<String>,
    pub enabled: Option<bool>,
    pub checked: Option<bool>,
    pub separator: bool,
    pub items: Vec<MenuEntry>,
}

#[derive(Debug, Deserialize)]
pub struct BindingsSection {
    pub path: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct InstallSection {
    pub command: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_unknown_keys() {
        let (_, unknown) = SabineFile::parse(
            "[app]\nnmae = \"Tern\"\n[web]\ndev_url = \"http://localhost:1\"\n[sound]\nvolume = 1",
        )
        .unwrap();
        assert_eq!(unknown, ["app.nmae", "sound"]);
    }

    #[test]
    fn reads_app_actions() {
        let (file, _) = SabineFile::parse(
            "[[app.actions]]\nid = \"new-window\"\nname = \"New Window\"\narguments = [\"--new-window\"]",
        )
        .unwrap();
        let action = &file.app.actions[0];
        assert_eq!(action.arguments, ["--new-window"]);
        assert!(AppAction::validate_all(&file.app.actions).is_ok());
        let invalid = AppAction {
            id: "new window".into(),
            ..action.clone()
        };
        assert!(AppAction::validate_all(&[invalid]).is_err());
        assert!(AppAction::validate_all(&[action.clone(), action.clone()]).is_err());
    }
}
