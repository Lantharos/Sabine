use std::{
    path::Path,
    process::{Command, Stdio},
};

use crate::types::{AppArtifactKind, PendingAppUpdate, ServiceError, ServiceResult};

#[cfg(target_os = "macos")]
use super::dmg::install_dmg;

pub(super) fn run_package_installer(
    update: &PendingAppUpdate,
    install_target: Option<&Path>,
) -> ServiceResult<()> {
    #[cfg(target_os = "linux")]
    if update.kind == AppArtifactKind::AppImage {
        return install_appimage(update, install_target);
    }
    #[cfg(target_os = "macos")]
    if update.kind == AppArtifactKind::Dmg {
        return install_dmg(update, install_target);
    }
    #[cfg(target_os = "windows")]
    let _ = install_target;
    #[cfg(target_os = "windows")]
    let _unchanged = hold_unchanged(update)?;
    let status = installer_command(update)?
        .stdin(Stdio::null())
        .status()
        .map_err(|error| ServiceError::Update(format!("failed to start installer: {error}")))?;
    let success = status.success()
        || (matches!(update.kind, AppArtifactKind::Msi)
            && status.code().is_some_and(|code| code == 3010));
    success
        .then_some(())
        .ok_or_else(|| ServiceError::Update(format!("installer exited with {status}")))
}

fn installer_command(update: &PendingAppUpdate) -> ServiceResult<Command> {
    match update.kind {
        #[cfg(target_os = "linux")]
        AppArtifactKind::Deb => Ok(elevated_command(update, &["apt-get", "install", "--yes"])),
        #[cfg(target_os = "linux")]
        AppArtifactKind::Rpm => Ok(elevated_command(update, rpm_installer())),
        #[cfg(target_os = "windows")]
        AppArtifactKind::Msi => {
            let mut command = Command::new("msiexec");
            command
                .arg("/i")
                .arg(&update.artifact)
                .args(["/passive", "/norestart"]);
            Ok(command)
        }
        #[cfg(target_os = "windows")]
        AppArtifactKind::Exe => {
            let mut command = Command::new(&update.artifact);
            command.arg("/S");
            Ok(command)
        }
        kind => Err(ServiceError::Update(format!(
            "{} updates are not installed by a package installer on this platform",
            kind.config_value()
        ))),
    }
}

/// Opens the installer so nobody can change, replace or move it until it has
/// run, and checks it, so the installer Windows elevates is the one that
/// matched its release.
#[cfg(target_os = "windows")]
fn hold_unchanged(update: &PendingAppUpdate) -> ServiceResult<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(&update.artifact)?;
    crate::download::verify_sha256(&update.artifact, &update.sha256)?;
    Ok(file)
}

#[cfg(target_os = "linux")]
fn rpm_installer() -> &'static [&'static str] {
    if sabine_runtime::find_program("dnf").is_some() {
        &["dnf", "install", "--assumeyes"]
    } else if sabine_runtime::find_program("zypper").is_some() {
        &[
            "zypper",
            "--non-interactive",
            "install",
            "--allow-unsigned-rpm",
        ]
    } else {
        &["rpm", "-U"]
    }
}

/// Copies the package somewhere only the administrator can write and checks
/// it there, so the file the package manager installs is the one that
/// matched its release, whatever happens to the download meanwhile.
#[cfg(target_os = "linux")]
const ELEVATED_INSTALL: &str = r#"artifact=$1 sha256=$2 extension=$3
shift 3
directory=$(mktemp -d) || exit 1
trap 'rm -rf "$directory"' EXIT
package="$directory/update.$extension"
cp -- "$artifact" "$package" || exit 1
if ! printf '%s  %s\n' "$sha256" "$package" | sha256sum --check --status; then
  echo "the update does not match its release" >&2
  exit 1
fi
"$@" "$package""#;

#[cfg(target_os = "linux")]
fn elevated_command(update: &PendingAppUpdate, installer: &[&str]) -> Command {
    let mut command = Command::new("pkexec");
    command
        .args(["/bin/sh", "-c", ELEVATED_INSTALL, "sh"])
        .arg(&update.artifact)
        .arg(&update.sha256)
        .arg(update.kind.config_value())
        .args(installer);
    command
}

#[cfg(target_os = "linux")]
fn install_appimage(update: &PendingAppUpdate, target: Option<&Path>) -> ServiceResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let target = target.ok_or_else(|| {
        ServiceError::Update("AppImage update is missing its installation path".to_string())
    })?;
    let temporary = target.with_extension("sabine-update");
    std::fs::copy(&update.artifact, &temporary)?;
    if let Err(error) = crate::download::verify_sha256(&temporary, &update.sha256) {
        std::fs::remove_file(&temporary)?;
        return Err(error);
    }
    let mut permissions = std::fs::metadata(&temporary)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&temporary, permissions)?;
    sabine_runtime::replace_file(&temporary, target)?;
    Ok(())
}
