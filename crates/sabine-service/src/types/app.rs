use serde::{Deserialize, Serialize};

use std::path::PathBuf;

use super::{
    SabineVersion, ServiceError, ServiceResult, is_https_url, unix_timestamp, valid_app_id,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UpdatePolicy {
    Disabled,
    Notify,
    #[default]
    Automatic,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppUpdateConfig {
    #[serde(flatten)]
    pub source: AppUpdateSource,
    #[serde(default = "stable_channel")]
    pub channel: String,
    #[serde(default)]
    pub policy: UpdatePolicy,
    #[serde(default)]
    pub install_mode: AppInstallMode,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub package_kind: Option<AppArtifactKind>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "provider", rename_all = "kebab-case")]
pub enum AppUpdateSource {
    Github { repository: String },
    Http { url: String },
}

impl AppUpdateSource {
    pub fn manifest_url(&self, channel: &str) -> ServiceResult<String> {
        match self {
            Self::Github { repository } => {
                if channel != "stable" {
                    return Err(ServiceError::InvalidManifest(
                        "GitHub updates currently support the stable channel only".to_string(),
                    ));
                }
                if !valid_github_repository(repository) {
                    return Err(ServiceError::InvalidManifest(
                        "GitHub repository must be in owner/name form".to_string(),
                    ));
                }
                Ok(format!(
                    "https://github.com/{repository}/releases/latest/download/sabine-update.json"
                ))
            }
            Self::Http { url } if is_https_url(url) => Ok(url.clone()),
            Self::Http { .. } => Err(ServiceError::InvalidManifest(
                "update manifests must use HTTPS".to_string(),
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AppInstallMode {
    #[default]
    Managed,
    Package,
    Store,
}

pub(crate) fn stable_channel() -> String {
    "stable".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub executable: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub update: Option<AppUpdateConfig>,
    #[serde(default)]
    pub sabine: SabineVersion,
}

impl AppManifest {
    pub fn validate(&self) -> ServiceResult<()> {
        if !valid_app_id(&self.id) {
            return Err(ServiceError::InvalidManifest(
                "id must contain only lowercase letters, digits, dots, and hyphens".to_string(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(ServiceError::InvalidManifest(
                "name is required".to_string(),
            ));
        }
        if self.version.trim().is_empty() {
            return Err(ServiceError::InvalidManifest(
                "version is required".to_string(),
            ));
        }
        if self.executable.as_os_str().is_empty() {
            return Err(ServiceError::InvalidManifest(
                "executable is required".to_string(),
            ));
        }
        if let Some(update) = &self.update {
            update.source.manifest_url(&update.channel)?;
            if update.install_mode != AppInstallMode::Store
                && update.policy != UpdatePolicy::Disabled
                && update.public_key.trim().is_empty()
            {
                return Err(ServiceError::InvalidManifest(
                    "enabled updates require an Ed25519 public key".to_string(),
                ));
            }
            if update.install_mode == AppInstallMode::Package && update.package_kind.is_none() {
                return Err(ServiceError::InvalidManifest(
                    "package updates require the installed package kind".to_string(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RegisteredApp {
    #[serde(flatten)]
    pub manifest: AppManifest,
    pub registered_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppReleaseManifest {
    #[serde(default = "release_schema")]
    pub schema: u32,
    pub app_id: String,
    pub version: String,
    #[serde(default = "stable_channel")]
    pub channel: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub requires_sabine: SabineVersion,
    pub artifacts: std::collections::BTreeMap<String, AppArtifact>,
    #[serde(default)]
    pub signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppArtifact {
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub kind: AppArtifactKind,
    #[serde(default)]
    pub executable: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AppArtifactKind {
    #[default]
    Archive,
    Deb,
    Rpm,
    Msi,
    Exe,
    Dmg,
    AppImage,
}

impl AppArtifactKind {
    pub fn requires_elevation(self) -> bool {
        matches!(self, Self::Deb | Self::Rpm | Self::Msi)
    }

    pub fn target_suffix(self) -> &'static str {
        match self {
            Self::Archive => "archive",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::Msi => "msi",
            Self::Exe => "exe",
            Self::Dmg => "dmg",
            Self::AppImage => "appimage",
        }
    }

    pub fn config_value(self) -> &'static str {
        match self {
            Self::AppImage => "app-image",
            _ => self.target_suffix(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PendingAppUpdate {
    pub app_id: String,
    pub version: String,
    pub artifact: PathBuf,
    pub sha256: String,
    pub kind: AppArtifactKind,
    pub requires_elevation: bool,
    #[serde(default)]
    pub staged_at: u64,
    #[serde(default)]
    pub prompt_after: u64,
}

impl PendingAppUpdate {
    pub fn ready_for_prompt(&self) -> bool {
        self.prompt_after <= unix_timestamp()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppUpdateStatus {
    Current,
    Deferred {
        version: String,
    },
    RequiresSystem {
        app_version: String,
        sabine: SabineVersion,
    },
    Installed {
        version: String,
    },
    PendingApproval(PendingAppUpdate),
    StoreManaged,
}

#[derive(Clone, Debug)]
pub struct MaintenanceReport {
    pub runtime: Option<sabine_runtime::RuntimeInfo>,
    pub pruned_runtimes: usize,
    pub registered_apps: usize,
    pub automatic_updates: usize,
    pub updated_apps: Vec<String>,
    pub pending_apps: Vec<String>,
    pub update_failures: Vec<String>,
    pub incompatible_apps: Vec<String>,
    pub required_system_update: Option<SabineVersion>,
}

fn release_schema() -> u32 {
    1
}

fn valid_github_repository(value: &str) -> bool {
    let Some((owner, name)) = value.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !name.is_empty()
        && !name.contains('/')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/'))
}
