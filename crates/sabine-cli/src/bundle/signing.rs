use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::{BundleFormat, stage::StagedBundle};

const DEFAULT_TIMESTAMP_URL: &str = "http://timestamp.digicert.com";

/// Code signing configured through the environment. Bundles are left unsigned
/// for each platform whose variables are absent.
pub(super) struct Signing {
    macos: Option<MacosSigning>,
    windows: Option<WindowsSigning>,
}

struct MacosSigning {
    identity: String,
    notary: Option<Notary>,
}

struct Notary {
    apple_id: String,
    team_id: String,
    password: String,
}

struct WindowsSigning {
    certificate: PathBuf,
    password: Option<String>,
    timestamp_url: String,
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
        let windows = variable("SABINE_WINDOWS_CERTIFICATE").map(|certificate| WindowsSigning {
            certificate: PathBuf::from(certificate),
            password: variable("SABINE_WINDOWS_CERTIFICATE_PASSWORD"),
            timestamp_url: variable("SABINE_WINDOWS_TIMESTAMP_URL")
                .unwrap_or_else(|| DEFAULT_TIMESTAMP_URL.to_string()),
        });
        Ok(Self { macos, windows })
    }

    /// Signs the staged application before it is packaged.
    pub(super) fn sign_payload(
        &self,
        format: BundleFormat,
        staged: &StagedBundle,
    ) -> Result<(), String> {
        match format {
            BundleFormat::Macos | BundleFormat::Dmg => {
                if let Some(macos) = &self.macos {
                    macos.sign(&staged.app_dir, true)?;
                }
            }
            BundleFormat::Windows | BundleFormat::Exe | BundleFormat::Msi => {
                if let Some(windows) = &self.windows {
                    for executable in executables(&staged.app_dir)? {
                        windows.sign(&executable)?;
                    }
                }
            }
            BundleFormat::Linux
            | BundleFormat::Portable
            | BundleFormat::Deb
            | BundleFormat::Rpm
            | BundleFormat::AppImage => {}
        }
        Ok(())
    }

    /// Signs installers and disk images, and notarizes disk images.
    pub(super) fn sign_artifacts(
        &self,
        format: BundleFormat,
        artifacts: &[PathBuf],
    ) -> Result<(), String> {
        match format {
            BundleFormat::Dmg => {
                if let Some(macos) = &self.macos {
                    for artifact in artifacts {
                        macos.sign(artifact, false)?;
                        if let Some(notary) = &macos.notary {
                            notary.notarize(artifact)?;
                        }
                    }
                }
            }
            BundleFormat::Exe | BundleFormat::Msi => {
                if let Some(windows) = &self.windows {
                    for artifact in artifacts {
                        windows.sign(artifact)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl MacosSigning {
    fn sign(&self, path: &Path, hardened: bool) -> Result<(), String> {
        let mut command = Command::new("codesign");
        command
            .args(["--force", "--timestamp", "--sign", &self.identity])
            .arg(path);
        if hardened {
            command.args(["--deep", "--options", "runtime"]);
        }
        run(&mut command, "codesign")
    }
}

impl Notary {
    fn notarize(&self, path: &Path) -> Result<(), String> {
        run(
            Command::new("xcrun")
                .args(["notarytool", "submit"])
                .arg(path)
                .args([
                    "--apple-id",
                    &self.apple_id,
                    "--team-id",
                    &self.team_id,
                    "--password",
                    &self.password,
                    "--wait",
                ]),
            "notarytool",
        )?;
        run(
            Command::new("xcrun").args(["stapler", "staple"]).arg(path),
            "stapler",
        )
    }
}

impl WindowsSigning {
    fn sign(&self, path: &Path) -> Result<(), String> {
        let mut command = Command::new("signtool");
        command
            .args([
                "sign",
                "/fd",
                "SHA256",
                "/td",
                "SHA256",
                "/tr",
                &self.timestamp_url,
                "/f",
            ])
            .arg(&self.certificate);
        if let Some(password) = &self.password {
            command.args(["/p", password]);
        }
        run(command.arg(path), "signtool")
    }
}

fn executables(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            found.extend(executables(&path)?);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            found.push(path);
        }
    }
    Ok(found)
}

fn variable(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn run(command: &mut Command, tool: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("failed to run {tool}: {error}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{tool} failed with {status}"))
}
