use std::{
    path::Path,
    process::{Command, Stdio},
};

use crate::types::{AppArtifactKind, PendingAppUpdate, ServiceError, ServiceResult};

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
        AppArtifactKind::Deb => Ok(elevated_command(
            "apt-get",
            &["install", "--yes"],
            &update.artifact,
        )),
        #[cfg(target_os = "linux")]
        AppArtifactKind::Rpm => Ok(if sabine_runtime::find_program("dnf").is_some() {
            elevated_command("dnf", &["install", "--assumeyes"], &update.artifact)
        } else {
            elevated_command("rpm", &["-U"], &update.artifact)
        }),
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

#[cfg(target_os = "linux")]
fn elevated_command(program: &str, args: &[&str], artifact: &Path) -> Command {
    let mut command = Command::new("pkexec");
    command.arg(program).args(args).arg(artifact);
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
    let mut permissions = std::fs::metadata(&temporary)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&temporary, permissions)?;
    crate::app::registry::replace_file(&temporary, target)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_dmg(update: &PendingAppUpdate, target: Option<&Path>) -> ServiceResult<()> {
    let executable = target.ok_or_else(|| {
        ServiceError::Update("DMG update is missing its application path".to_string())
    })?;
    let app = executable
        .ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .ok_or_else(|| ServiceError::Update("could not locate installed macOS app".to_string()))?;
    let mount = update.artifact.with_extension("mount");
    if mount.exists() {
        let _ = std::fs::remove_dir_all(&mount);
    }
    std::fs::create_dir_all(&mount)?;
    let attach = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mount)
        .arg(&update.artifact)
        .status()?;
    if !attach.success() {
        return Err(ServiceError::Update(
            "could not mount app update".to_string(),
        ));
    }
    let result = (|| {
        let source = std::fs::read_dir(&mount)?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|extension| extension == "app"))
            .ok_or_else(|| {
                ServiceError::Update("DMG contains no application bundle".to_string())
            })?;
        let source = shell_single_quote(&source.display().to_string());
        let destination = shell_single_quote(&app.display().to_string());
        let staged = shell_single_quote(&format!("{}.sabine-new", app.display()));
        let backup = shell_single_quote(&format!("{}.sabine-old", app.display()));
        let script = format!(
            "/bin/rm -rf {staged} {backup} && /usr/bin/ditto {source} {staged} && /bin/mv {destination} {backup} && (/bin/mv {staged} {destination} && /bin/rm -rf {backup} || (/bin/mv {backup} {destination}; exit 1))"
        );
        let status = if replaceable_by_user(app) {
            Command::new("/bin/sh").arg("-c").arg(&script).status()?
        } else {
            Command::new("osascript")
                .arg("-e")
                .arg(format!(
                    "do shell script {} with administrator privileges",
                    apple_script_string(&script)
                ))
                .status()?
        };
        status
            .success()
            .then_some(())
            .ok_or_else(|| ServiceError::Update("macOS app installation was cancelled".to_string()))
    })();
    let _ = Command::new("hdiutil").arg("detach").arg(&mount).status();
    let _ = std::fs::remove_dir_all(mount);
    result
}

/// Whether the current user can swap the bundle in place: they own it and may
/// write to the folder holding it, as with apps dragged into `/Applications`.
#[cfg(target_os = "macos")]
fn replaceable_by_user(app: &Path) -> bool {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    let user = unsafe { libc::getuid() };
    let owned = std::fs::symlink_metadata(app).is_ok_and(|metadata| metadata.uid() == user);
    let writable_parent = app
        .parent()
        .and_then(|parent| std::ffi::CString::new(parent.as_os_str().as_bytes()).ok())
        .is_some_and(|parent| unsafe { libc::access(parent.as_ptr(), libc::W_OK) } == 0);
    owned && writable_parent
}

#[cfg(target_os = "macos")]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn apple_script_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
