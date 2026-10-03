mod app;
mod system;

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

pub use app::{
    APPIMAGE_PROGRAM_ENV, AppArtifact, AppArtifactKind, AppInstallMode, AppManifest,
    AppReleaseManifest, AppUpdateConfig, AppUpdateSource, AppUpdateStatus, MaintenanceReport,
    PendingAppUpdate, RegisteredApp, UpdatePolicy,
};
pub use system::{
    SabineVersion, SystemCompatibility, SystemReleaseArtifact, SystemReleaseManifest,
};

pub const REGISTRY_VERSION: u32 = 1;
pub const SABINE_VERSION: &str = "0.33";
pub const SABINE_MAJOR: u32 = 0;
pub const SABINE_BUILD: u32 = 33;
pub const MIN_SUPPORTED_APP_BUILD: u32 = 30;
pub const UPDATE_SOAK: Duration = Duration::from_secs(24 * 60 * 60);
pub const UPDATE_ROLLOUT_WINDOW: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid app manifest: {0}")]
    InvalidManifest(String),
    #[error("app `{0}` is not registered")]
    AppNotFound(String),
    #[error("{message}")]
    IncompatibleApp { app_id: String, message: String },
    #[error("runtime operation failed: {0}")]
    Runtime(#[from] sabine_runtime::RuntimeError),
    #[error("app update failed: {0}")]
    Update(String),
    #[error("could not decode {path}: {source}")]
    Decode {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

pub fn service_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join("Sabine");
    }
    #[cfg(target_os = "macos")]
    if let Some(path) = std::env::var_os("HOME") {
        return PathBuf::from(path)
            .join("Library")
            .join("Application Support")
            .join("Sabine");
    }
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        return PathBuf::from(path).join("sabine");
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(home).join(".local/share/sabine")
}

pub fn default_maintenance_interval() -> Duration {
    Duration::from_secs(6 * 60 * 60)
}

pub fn valid_app_id(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

pub(crate) fn is_https_url(value: &str) -> bool {
    value.starts_with("https://") && value.len() > "https://".len()
}

pub(crate) fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn platform_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        ("macos", "aarch64") => "macos-aarch64",
        _ => "unsupported",
    }
}

pub(crate) fn update_artifact_target(
    install_mode: AppInstallMode,
    kind: Option<AppArtifactKind>,
) -> String {
    if install_mode == AppInstallMode::Package
        && let Some(kind) = kind
    {
        return format!("{}-{}", platform_target(), kind.target_suffix());
    }
    platform_target().to_string()
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
