use std::path::{Path, PathBuf};

use crate::{PrepareProgress, PrepareStage, ServiceError, ServiceResult, SystemReleaseManifest};

use super::SERVICE_REPO;

pub(super) fn system_asset_name() -> String {
    let extension = if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.gz"
    };
    format!(
        "sabine-system-{}.{extension}",
        crate::types::PLATFORM_TARGET
    )
}

pub(super) fn sabine_host_relative_path() -> PathBuf {
    if cfg!(target_os = "windows") {
        PathBuf::from("sabine-host.exe")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("sabine-host.app/Contents/MacOS/sabine-host")
    } else {
        PathBuf::from("sabine-host")
    }
}

pub(super) fn cli_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "sabine.exe"
    } else {
        "sabine"
    }
}

pub(super) fn service_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "sabine-service.exe"
    } else {
        "sabine-service"
    }
}

pub(super) fn service_daemon_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "sabine-service-daemon.exe"
    } else {
        "sabine-service-daemon"
    }
}

pub(super) fn download_system_artifact(
    url: &str,
    destination: &Path,
    size: u64,
    on_progress: &mut impl FnMut(PrepareProgress),
) -> ServiceResult<()> {
    crate::download::download_artifact(url, destination, Some(size), |downloaded| {
        on_progress(PrepareProgress {
            stage: PrepareStage::Service,
            message: "Downloading Sabine service".to_string(),
            fraction: Some(0.02 + downloaded as f32 / size as f32 * 0.06),
        })
    })
}

pub(super) fn fetch_system_manifest(
    required_version: Option<&str>,
) -> ServiceResult<SystemReleaseManifest> {
    let url = std::env::var("SABINE_RELEASE_MANIFEST_URL")
        .unwrap_or_else(|_| system_manifest_url(required_version));
    if !url.starts_with("https://") {
        return Err(ServiceError::Update(
            "Sabine release manifest must use HTTPS".to_string(),
        ));
    }
    crate::download::fetch_manifest(&url)
}

fn system_manifest_url(required_version: Option<&str>) -> String {
    required_version.map_or_else(
        || {
            format!(
                "https://github.com/{SERVICE_REPO}/releases/latest/download/sabine-release.json"
            )
        },
        |version| {
            format!(
                "https://github.com/{SERVICE_REPO}/releases/download/v{version}/sabine-release.json"
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn required_updates_keep_the_exact_release_tag() {
        assert!(system_manifest_url(None).contains("/releases/latest/"));
        assert!(system_manifest_url(Some("0.21")).contains("/releases/download/v0.21/"));
    }
}
