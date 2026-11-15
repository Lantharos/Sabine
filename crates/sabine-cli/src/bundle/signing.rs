use std::{env, path::Path, process::Command};

use super::{
    BuildTarget, BundleFormat,
    macos::signing::{MacosSigning, Notary},
    stage::StagedBundle,
    windows::signing::WindowsSigning,
};

const DEFAULT_TIMESTAMP_URL: &str = "http://timestamp.digicert.com";

/// Code signing configured through the environment for the operating system
/// a bundle runs on. Bundles stay unsigned when its variables are absent.
pub(super) enum Signing {
    Macos(MacosSigning),
    Windows(WindowsSigning),
    Unsigned,
}

impl Signing {
    pub(super) fn from_env(target: BuildTarget) -> Result<Self, String> {
        Ok(match target {
            BuildTarget::Macos => macos()?.map_or(Self::Unsigned, Self::Macos),
            BuildTarget::Windows => windows()?.map_or(Self::Unsigned, Self::Windows),
            BuildTarget::Linux => Self::Unsigned,
        })
    }

    pub(super) fn windows(&self) -> Option<&WindowsSigning> {
        match self {
            Self::Windows(windows) => Some(windows),
            _ => None,
        }
    }

    /// Signs the staged application before it is packaged. An app archived
    /// as it is, rather than inside a disk image, is also notarized.
    pub(super) fn sign_payload(
        &self,
        format: BundleFormat,
        staged: &StagedBundle,
    ) -> Result<(), String> {
        match self {
            Self::Macos(macos) => {
                macos.sign_app(&staged.app_dir)?;
                if format != BundleFormat::Dmg {
                    macos.notarize_app(&staged.app_dir)?;
                }
                Ok(())
            }
            Self::Windows(windows) => windows.sign_app(&staged.app_dir),
            Self::Unsigned => Ok(()),
        }
    }

    /// Signs installers and disk images, and notarizes disk images.
    pub(super) fn sign_artifact(
        &self,
        format: BundleFormat,
        artifact: &Path,
    ) -> Result<(), String> {
        match (format, self) {
            (BundleFormat::Dmg, Self::Macos(macos)) => macos.sign_disk_image(artifact),
            (BundleFormat::Exe | BundleFormat::Msi, Self::Windows(windows)) => {
                windows.sign(&[artifact.to_path_buf()])
            }
            _ => Ok(()),
        }
    }
}

fn macos() -> Result<Option<MacosSigning>, String> {
    let api_key = match (
        variable("SABINE_NOTARY_API_KEY"),
        variable("SABINE_NOTARY_API_KEY_ID"),
        variable("SABINE_NOTARY_API_ISSUER"),
    ) {
        (Some(key), Some(key_id), Some(issuer)) => Some(Notary::ApiKey {
            key: key.into(),
            key_id,
            issuer,
        }),
        (None, None, None) => None,
        _ => {
            return Err("notarization needs SABINE_NOTARY_API_KEY, SABINE_NOTARY_API_KEY_ID and SABINE_NOTARY_API_ISSUER together".into());
        }
    };
    let notary = match (api_key, variable("SABINE_NOTARY_KEYCHAIN_PROFILE")) {
        (Some(_), Some(_)) => {
            return Err("set the SABINE_NOTARY_API_KEY variables or SABINE_NOTARY_KEYCHAIN_PROFILE, not both".into());
        }
        (api_key, profile) => api_key.or(profile.map(Notary::KeychainProfile)),
    };
    match (variable("SABINE_MACOS_SIGNING_IDENTITY"), notary) {
        (Some(identity), notary) => Ok(Some(MacosSigning { identity, notary })),
        (None, Some(_)) => Err("notarization needs SABINE_MACOS_SIGNING_IDENTITY".into()),
        (None, None) => Ok(None),
    }
}

fn windows() -> Result<Option<WindowsSigning>, String> {
    let timestamp_url = variable("SABINE_WINDOWS_TIMESTAMP_URL")
        .unwrap_or_else(|| DEFAULT_TIMESTAMP_URL.to_string());
    match (
        variable("SABINE_WINDOWS_CERTIFICATE"),
        variable("SABINE_WINDOWS_CERTIFICATE_SHA1"),
    ) {
        (Some(_), Some(_)) => Err(
            "set SABINE_WINDOWS_CERTIFICATE or SABINE_WINDOWS_CERTIFICATE_SHA1, not both".into(),
        ),
        (Some(path), None) => WindowsSigning::from_file(
            Path::new(&path),
            variable("SABINE_WINDOWS_CERTIFICATE_PASSWORD").as_deref(),
            timestamp_url,
        )
        .map(Some),
        (None, Some(thumbprint)) => WindowsSigning::from_store(thumbprint, timestamp_url).map(Some),
        (None, None) => Ok(None),
    }
}

fn variable(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

pub(super) fn run(command: &mut Command, tool: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("failed to run {tool}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{tool} failed with {status}"))
}
