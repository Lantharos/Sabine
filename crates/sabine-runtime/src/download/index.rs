use std::{collections::BTreeMap, path::PathBuf};

use serde::Deserialize;

use crate::discovery::paths::runtime_version_path;
use crate::discovery::version::{cef_platform_key, major_version, version_sort_key};
use crate::download::http::fetch_bytes;
use crate::error::RuntimeError;

const CEF_BUILDS: &str = "https://cef-builds.spotifycdn.com";

/// The CEF build index lists every build of every platform, past ten megabytes.
const INDEX_LIMIT: u64 = 64 * 1024 * 1024;

const ARCHIVE_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Deserialize)]
struct CefIndex {
    #[serde(flatten)]
    platforms: BTreeMap<String, CefPlatform>,
}

#[derive(Deserialize)]
struct CefPlatform {
    versions: Vec<CefBuild>,
}

#[derive(Deserialize)]
struct CefBuild {
    cef_version: String,
    channel: Option<String>,
    files: Vec<CefFile>,
}

#[derive(Deserialize)]
struct CefFile {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    size: u64,
    sha1: String,
}

pub(crate) struct RuntimeInstallPlan {
    pub version: String,
    pub archive_name: String,
    pub archive_size: u64,
    pub url: String,
    pub sha1: String,
    pub install_dir: PathBuf,
}

/// The newest stable CEF build with a Minimal archive for this platform, as
/// CEF's build index lists it.
pub(crate) fn latest_install_plan() -> Result<RuntimeInstallPlan, RuntimeError> {
    let platform = cef_platform_key().ok_or_else(|| {
        RuntimeError::InstallationFailed(format!(
            "CEF has no builds for {} on {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    let mut index = fetch_index()?;
    let builds = index.platforms.remove(platform).ok_or_else(|| {
        RuntimeError::InstallationFailed(format!("the CEF build index has no {platform} builds"))
    })?;
    newest_stable(builds, platform)
}

fn fetch_index() -> Result<CefIndex, RuntimeError> {
    let index = fetch_bytes(&format!("{CEF_BUILDS}/index.json"), INDEX_LIMIT)?;
    serde_json::from_slice(&index).map_err(|error| {
        RuntimeError::InstallationFailed(format!("invalid CEF build index: {error}"))
    })
}

fn newest_stable(builds: CefPlatform, platform: &str) -> Result<RuntimeInstallPlan, RuntimeError> {
    let (version, file) = builds
        .versions
        .into_iter()
        .filter(|build| build.channel.as_deref().unwrap_or("stable") == "stable")
        .filter(|build| major_version(&build.cef_version) >= crate::MIN_CEF_MAJOR)
        .filter_map(|build| {
            let minimal = build
                .files
                .into_iter()
                .find(|file| file.kind == "minimal")?;
            Some((build.cef_version, minimal))
        })
        .max_by_key(|(version, _)| version_sort_key(version))
        .ok_or_else(|| {
            RuntimeError::NotFound(format!(
                "CEF has no stable Minimal build for {platform} from CEF {} on",
                crate::MIN_CEF_MAJOR
            ))
        })?;
    if !safe_name(&version)
        || !safe_name(&file.name)
        || file.size == 0
        || file.size > ARCHIVE_LIMIT
        || file.sha1.len() != 40
        || !file.sha1.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(RuntimeError::InstallationFailed(
            "the CEF build index lists an invalid archive".to_string(),
        ));
    }
    Ok(RuntimeInstallPlan {
        install_dir: runtime_version_path(&version),
        url: format!("{CEF_BUILDS}/{}", file.name),
        version,
        archive_name: file.name,
        archive_size: file.size,
        sha1: file.sha1,
    })
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '+' | '-' | '_'))
}
