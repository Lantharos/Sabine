use std::{collections::BTreeMap, fs, path::Path};

use sabine_service::SandboxSection;
use serde::Deserialize;

use crate::desktop::{entry::Listing, types::Associations};

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SabineFile {
    #[serde(default)]
    pub(crate) app: AppSection,
    #[serde(default)]
    pub(crate) web: WebSection,
    #[serde(default)]
    pub(crate) window: WindowSection,
    pub(crate) updates: Option<sabine_service::AppUpdatesSection>,
    pub(crate) sandbox: Option<SandboxSection>,
    #[serde(default)]
    pub(crate) resources: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) install: InstallSection,
    pub(crate) bindings: Option<BindingsSection>,
}

impl SabineFile {
    /// The Cargo manifest of the app's executable.
    pub(crate) fn cargo_manifest(&self, source_dir: &Path) -> std::path::PathBuf {
        source_dir.join(self.app.cargo_manifest.as_deref().unwrap_or("Cargo.toml"))
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct AppSection {
    pub(crate) id: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) publisher: Option<String>,
    pub(crate) maintainer: Option<String>,
    pub(crate) license: Option<String>,
    pub(crate) homepage: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) icon: Option<String>,
    pub(crate) installer_image: Option<String>,
    pub(crate) shared_runtime: Option<bool>,
    #[serde(flatten)]
    pub(crate) associations: Associations,
    #[serde(flatten)]
    pub(crate) listing: Listing,
    pub(crate) cargo_manifest: Option<String>,
    pub(crate) command: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct WebSection {
    pub(crate) root: Option<String>,
    pub(crate) dist: Option<String>,
    pub(crate) entry: Option<String>,
    pub(crate) build: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) dev_url: Option<String>,
    pub(crate) dev_port: Option<u16>,
    pub(crate) dev_command: Option<String>,
    #[serde(default)]
    pub(crate) allowed_origins: Vec<String>,
    #[serde(default)]
    pub(crate) local_files: bool,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct WindowSection {
    #[serde(default)]
    pub splash: bool,
    pub splash_delay_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BindingsSection {
    pub(crate) path: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct InstallSection {
    pub(crate) command: Option<String>,
}

/// `Sabine.toml` in `source_dir`, or the defaults when there is none.
pub(crate) fn read_sabine_file(source_dir: &Path) -> Result<SabineFile, String> {
    let path = source_dir.join("Sabine.toml");
    if !path.exists() {
        return Ok(SabineFile::default());
    }
    let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    toml::from_str(&text).map_err(|error| format!("failed to parse {}: {error}", path.display()))
}
