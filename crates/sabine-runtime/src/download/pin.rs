use std::{collections::BTreeMap, path::PathBuf};

use serde::Deserialize;

use crate::discovery::paths::runtime_version_path;
use crate::discovery::version::cef_platform_key;
use crate::error::RuntimeError;

const CEF_BUILDS: &str = "https://cef-builds.spotifycdn.com";

/// The CEF build this Sabine installs, with the size and SHA-256 of each
/// platform's Minimal archive. `scripts/cef/pin-cef.py` moves it to a newer build.
const PINNED_BUILD: &str = include_str!("cef.json");

#[derive(Deserialize)]
struct PinnedBuild {
    version: String,
    archives: BTreeMap<String, PinnedArchive>,
}

#[derive(Deserialize)]
struct PinnedArchive {
    name: String,
    size: u64,
    sha256: String,
}

pub(crate) struct RuntimeInstallPlan {
    pub version: String,
    pub archive_name: String,
    pub archive_size: u64,
    pub url: String,
    pub sha256: String,
    pub install_dir: PathBuf,
}

fn pinned_build() -> PinnedBuild {
    serde_json::from_str(PINNED_BUILD).expect("the pinned CEF build is valid")
}

pub(crate) fn pinned_install_plan() -> Result<RuntimeInstallPlan, RuntimeError> {
    let mut build = pinned_build();
    let archive = cef_platform_key()
        .and_then(|platform| build.archives.remove(platform))
        .ok_or_else(|| {
            RuntimeError::InstallationFailed(format!(
                "no CEF build is pinned for {} on {}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ))
        })?;
    Ok(RuntimeInstallPlan {
        install_dir: runtime_version_path(&build.version),
        version: build.version,
        url: format!("{CEF_BUILDS}/{}", archive.name),
        archive_name: archive.name,
        archive_size: archive.size,
        sha256: archive.sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_build_covers_every_supported_platform() {
        let build = pinned_build();
        assert!(crate::discovery::version::version_satisfies(
            &build.version,
            crate::MIN_CEF_MAJOR
        ));
        for platform in [
            "linux64",
            "linuxarm64",
            "windows64",
            "windowsarm64",
            "macosarm64",
        ] {
            let archive = &build.archives[platform];
            assert_eq!(
                archive.name,
                format!("cef_binary_{}_{platform}_minimal.tar.bz2", build.version)
            );
            assert!(archive.size > 0);
            assert!(
                archive.sha256.len() == 64
                    && archive.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            );
        }
    }
}
