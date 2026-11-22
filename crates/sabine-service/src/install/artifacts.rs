use std::path::{Path, PathBuf};

use crate::types::is_https_url;
use crate::{
    PrepareProgress, PrepareStage, SabineVersion, ServiceError, ServiceResult,
    SystemReleaseManifest,
};

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

/// The signed manifest of Sabine `required_version`, or of the newest
/// release, once its signature and contents check out.
pub(super) fn fetch_system_manifest(
    required_version: Option<&str>,
) -> ServiceResult<SystemReleaseManifest> {
    let url = manifest_override().unwrap_or_else(|| system_manifest_url(required_version));
    if !is_https_url(&url) {
        return Err(ServiceError::Update(
            "Sabine release manifest must use HTTPS".to_string(),
        ));
    }
    let manifest = crate::download::fetch_manifest(&url)?;
    super::validate_system_release(&manifest)?;
    Ok(manifest)
}

/// The signed manifest of the newest published release.
pub(super) fn newest_release() -> ServiceResult<SystemReleaseManifest> {
    if manifest_override().is_some() {
        return fetch_system_manifest(None);
    }
    let feed = crate::download::fetch_release_document(&format!(
        "https://github.com/{SERVICE_REPO}/releases.atom"
    ))?;
    let version = released_versions(&String::from_utf8_lossy(&feed))
        .max()
        .ok_or_else(|| ServiceError::Update("no published Sabine release found".into()))?;
    fetch_system_manifest(Some(&version.label()))
}

fn released_versions(feed: &str) -> impl Iterator<Item = SabineVersion> + '_ {
    feed.split("<id>tag:github.com,2008:Repository/")
        .skip(1)
        .filter_map(|entry| {
            let (id, _) = entry.split_once("</id>")?;
            SabineVersion::parse(id.split_once('/')?.1)
        })
}

pub(super) fn manifest_override() -> Option<String> {
    std::env::var("SABINE_RELEASE_MANIFEST_URL").ok()
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

    #[test]
    fn the_release_feed_names_each_published_version() {
        let feed = r#"<feed><id>tag:github.com,2008:https://github.com/Lantharos/Sabine/releases</id>
            <entry><id>tag:github.com,2008:Repository/1259393564/v0.105</id><title>v0.105</title></entry>
            <entry><id>tag:github.com,2008:Repository/1259393564/v0.99</id><title>v0.99</title></entry></feed>"#;
        assert_eq!(
            released_versions(feed).collect::<Vec<_>>(),
            [
                SabineVersion {
                    major: 0,
                    build: 105
                },
                SabineVersion {
                    major: 0,
                    build: 99
                }
            ]
        );
    }
}
