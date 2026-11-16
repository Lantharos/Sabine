mod app;
pub mod manifest;
mod permissions;
mod spellcheck;
mod system;
mod updates_section;

use std::{
    path::{Component, Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub use app::{
    APPIMAGE_PROGRAM_ENV, AppArtifact, AppArtifactKind, AppInstallMode, AppManifest,
    AppReleaseManifest, AppUpdateConfig, AppUpdateSource, AppUpdateStatus, MaintenanceReport,
    PendingAppUpdate, RegisteredApp, UpdatePolicy,
};
pub use permissions::{DeclaredPermission, Permission, PermissionsSection};
pub use spellcheck::{SpellcheckSection, validate_spellcheck_languages};
pub use system::{
    HeldSystemUpdate, InstalledApp, SabineVersion, SystemCompatibility, SystemReleaseArtifact,
    SystemReleaseManifest, SystemRequirement,
};
pub use updates_section::AppUpdatesSection;

pub const REGISTRY_VERSION: u32 = 1;
pub const SABINE_VERSION: &str = "0.96";
pub const SABINE_MAJOR: u32 = 0;
pub const SABINE_BUILD: u32 = 96;
pub const MIN_SUPPORTED_APP_BUILD: u32 = 91;
pub const UPDATE_SOAK: Duration = Duration::from_secs(24 * 60 * 60);
pub const UPDATE_ROLLOUT_WINDOW: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid app manifest: {0}")]
    InvalidManifest(String),
    #[error("app `{0}` is not registered")]
    AppNotFound(String),
    #[error("{0}")]
    IncompatibleApp(String),
    #[error("runtime operation failed: {0}")]
    Runtime(#[from] sabine_runtime::RuntimeError),
    #[error("app update failed: {0}")]
    Update(String),
    #[error("{0}")]
    Gone(String),
    #[error("could not decode {path}: {source}")]
    Decode {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

pub fn default_maintenance_interval() -> Duration {
    Duration::from_secs(6 * 60 * 60)
}

pub fn valid_app_id(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

pub(crate) fn is_https_url(value: &str) -> bool {
    value.starts_with("https://") && value.len() > "https://".len()
}

/// Whether `path` names something inside the folder it is joined to: relative,
/// with only plain names, and none a Windows stream or drive.
pub(crate) fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && !path.as_os_str().to_string_lossy().contains(':')
}

pub(crate) fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) const PLATFORM_TARGET: &str = "linux-x86_64";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub(crate) const PLATFORM_TARGET: &str = "linux-aarch64";
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub(crate) const PLATFORM_TARGET: &str = "windows-x86_64";
#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
pub(crate) const PLATFORM_TARGET: &str = "windows-aarch64";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) const PLATFORM_TARGET: &str = "macos-aarch64";

pub(crate) fn update_artifact_target(
    install_mode: AppInstallMode,
    kind: Option<AppArtifactKind>,
) -> String {
    if install_mode == AppInstallMode::Package
        && let Some(kind) = kind
    {
        return format!("{PLATFORM_TARGET}-{}", kind.target_suffix());
    }
    PLATFORM_TARGET.to_string()
}

/// Whether `value` is a version Sabine can order, which also makes it a
/// single plain path component.
pub(crate) fn valid_version(value: &str) -> bool {
    parse_semver(value).is_ok()
}

pub(crate) fn version_is_newer(candidate: &str, current: &str) -> bool {
    parse_semver(candidate)
        .ok()
        .zip(parse_semver(current).ok())
        .is_some_and(|(candidate, current)| candidate > current)
}

fn parse_semver(value: &str) -> Result<semver::Version, semver::Error> {
    let value = value.trim_start_matches('v');
    if value.bytes().filter(|byte| *byte == b'.').count() == 1 {
        semver::Version::parse(&format!("{value}.0"))
    } else {
        semver::Version::parse(value)
    }
}
