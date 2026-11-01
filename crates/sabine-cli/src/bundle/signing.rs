use std::{env, path::Path, process::Command};

use super::{
    BuildTarget, BundleFormat,
    macos::signing::{MacosSigning, Notary},
    stage::StagedBundle,
    windows::signing::{WindowsCertificate, WindowsSigning},
};

const DEFAULT_TIMESTAMP_URL: &str = "http://timestamp.digicert.com";

/// Code signing configured through the environment. Bundles are left unsigned
/// for each platform whose variables are absent.
pub(super) struct Signing {
    macos: Option<MacosSigning>,
    windows: Option<WindowsSigning>,
}

impl Signing {
    pub(super) fn from_env() -> Result<Self, String> {
        let notary = match (
            variable("SABINE_NOTARY_APPLE_ID"),
            variable("SABINE_NOTARY_TEAM_ID"),
            variable("SABINE_NOTARY_PASSWORD"),
        ) {
            (Some(apple_id), Some(team_id), Some(password)) => Some(Notary {
                apple_id,
                team_id,
                password,
            }),
            (None, None, None) => None,
            _ => {
                return Err("notarization needs SABINE_NOTARY_APPLE_ID, SABINE_NOTARY_TEAM_ID and SABINE_NOTARY_PASSWORD together".into());
            }
        };
        let macos = variable("SABINE_MACOS_SIGNING_IDENTITY")
            .map(|identity| MacosSigning { identity, notary });
        if macos.is_none() && variable("SABINE_NOTARY_APPLE_ID").is_some() {
            return Err("notarization needs SABINE_MACOS_SIGNING_IDENTITY".into());
        }
        let certificate = match (
            variable("SABINE_WINDOWS_CERTIFICATE"),
            variable("SABINE_WINDOWS_CERTIFICATE_SHA1"),
        ) {
            (Some(_), Some(_)) => {
                return Err(
                    "set SABINE_WINDOWS_CERTIFICATE or SABINE_WINDOWS_CERTIFICATE_SHA1, not both"
                        .into(),
                );
            }
            (Some(path), None) => Some(WindowsCertificate::File {
                path: path.into(),
                password: variable("SABINE_WINDOWS_CERTIFICATE_PASSWORD"),
            }),
            (None, Some(thumbprint)) => Some(WindowsCertificate::Store { thumbprint }),
            (None, None) => None,
        };
        let windows = certificate.map(|certificate| WindowsSigning {
            certificate,
            timestamp_url: variable("SABINE_WINDOWS_TIMESTAMP_URL")
                .unwrap_or_else(|| DEFAULT_TIMESTAMP_URL.to_string()),
        });
        Ok(Self { macos, windows })
    }

    /// Signs the staged application before it is packaged. An app archived
    /// as it is, rather than inside a disk image, is also notarized.
    pub(super) fn sign_payload(
        &self,
        format: BundleFormat,
        staged: &StagedBundle,
    ) -> Result<(), String> {
        match format.target() {
            BuildTarget::Macos => {
                if let Some(macos) = &self.macos {
                    macos.sign_app(&staged.app_dir)?;
                    if format != BundleFormat::Dmg {
                        macos.notarize_app(&staged.app_dir)?;
                    }
                }
            }
            BuildTarget::Windows => {
                if let Some(windows) = &self.windows {
                    windows.sign_app(&staged.app_dir)?;
                }
            }
            BuildTarget::Linux => {}
        }
        Ok(())
    }

    /// Signs installers and disk images, and notarizes disk images.
    pub(super) fn sign_artifact(
        &self,
        format: BundleFormat,
        artifact: &Path,
    ) -> Result<(), String> {
        match (format, &self.macos, &self.windows) {
            (BundleFormat::Dmg, Some(macos), _) => macos.sign_disk_image(artifact),
            (BundleFormat::Exe | BundleFormat::Msi, _, Some(windows)) => {
                windows.sign(&[artifact.to_path_buf()])
            }
            _ => Ok(()),
        }
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
