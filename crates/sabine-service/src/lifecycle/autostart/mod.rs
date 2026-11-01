use std::{
    path::Path,
    process::{Command, Stdio},
};

use crate::{ServiceError, ServiceResult, ensure_service_executable, service_daemon_path};
use sabine_runtime::configure_background_command;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

#[cfg(target_os = "linux")]
mod systemd_unit;

#[cfg(target_os = "macos")]
pub(super) use platform::{launch_outside_job, unload_macos_daemon};
#[cfg(target_os = "linux")]
pub(super) use platform::{
    launch_outside_unit, retire_shadowing_user_unit, supervised_daemon_matches,
};
#[cfg(target_os = "linux")]
pub use systemd_unit::install_system_unit;

pub fn install_login_autostart() -> ServiceResult<bool> {
    let executable = ensure_service_executable(|_| {})?;
    install_login_autostart_with(&executable)
}

/// Starts the daemon beside `executable` at every login. Returns whether the
/// operating system's service manager is also running it now; when it is not,
/// the caller starts the daemon itself.
pub fn install_login_autostart_with(executable: &Path) -> ServiceResult<bool> {
    let daemon = service_daemon_path(executable);
    if !daemon.is_file() {
        return Err(ServiceError::Update(format!(
            "Sabine service daemon not found at {}",
            daemon.display()
        )));
    }
    platform::install(executable, &daemon)
}

pub fn uninstall_login_autostart() -> ServiceResult<()> {
    platform::uninstall()
}

/// Runs a helper quietly; their output would otherwise reach the setup window.
pub(in crate::lifecycle) fn run_checked(command: &mut Command) -> ServiceResult<()> {
    configure_background_command(command);
    let status = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(ServiceError::Update(format!(
            "command failed with {status}"
        )))
    }
}
