use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use super::AppUpdatesSection;
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
    pub resources: BTreeMap<String, String>,
    #[serde(default)]
    pub install: InstallSection,
    pub bindings: Option<BindingsSection>,
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
    pub cargo_manifest: Option<String>,
    pub command: Option<String>,
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
}
