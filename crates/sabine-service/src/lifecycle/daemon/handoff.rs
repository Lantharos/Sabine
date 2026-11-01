use std::{path::Path, time::Duration};

use crate::{
    ServiceError, ServiceResult,
    install::{StagedSystemUpdate, cached_service_path, rollback_system_update},
};

use super::{
    install_login_autostart_with, load_policy, process_alive, service_version, start_daemon_at,
    stop_daemon, wait_for_daemon_version,
};

/// Starts the helper that replaces this daemon with the staged update once
/// this daemon has exited. The helper runs outside the service manager's job
/// for this daemon, which ends with it.
pub(super) fn begin_system_handoff(update: &StagedSystemUpdate) -> ServiceResult<()> {
    let Some(helper) = update.previous_service.as_ref() else {
        let _ = rollback_system_update(&update.version);
        return Err(ServiceError::Update(
            "self-update has no running installation to perform handoff".into(),
        ));
    };
    let arguments = [
        "complete-system-update".to_string(),
        "--from-pid".to_string(),
        std::process::id().to_string(),
        "--version".to_string(),
        update.version.clone(),
    ];
    if let Err(error) = launch_helper(helper, &arguments) {
        let _ = rollback_system_update(&update.version);
        return Err(ServiceError::Update(format!(
            "failed to start Sabine update handoff: {error}"
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn launch_helper(helper: &Path, arguments: &[String]) -> ServiceResult<()> {
    super::super::autostart::launch_outside_unit(helper, arguments)
}

#[cfg(target_os = "macos")]
fn launch_helper(helper: &Path, arguments: &[String]) -> ServiceResult<()> {
    super::super::autostart::launch_outside_job(helper, arguments)
}

#[cfg(target_os = "windows")]
fn launch_helper(helper: &Path, arguments: &[String]) -> ServiceResult<()> {
    use std::process::Stdio;
    sabine_runtime::background_command(helper)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

pub fn complete_system_update(from_pid: u32, version: &str) -> ServiceResult<()> {
    #[cfg(target_os = "macos")]
    super::super::autostart::unload_macos_daemon();
    while process_alive(from_pid) {
        std::thread::sleep(Duration::from_millis(100));
    }
    start_updated_daemon(&cached_service_path())?;
    if wait_for_daemon_version(version, Duration::from_secs(20)) {
        return Ok(());
    }

    stop_failed_handoff_daemon();
    let previous = rollback_system_update(version)?.ok_or_else(|| {
        ServiceError::Update(format!(
            "Sabine {version} did not start and no rollback installation is available"
        ))
    })?;
    start_updated_daemon(&previous)?;
    let previous_version = service_version(&previous).unwrap_or_default();
    if !wait_for_daemon_version(&previous_version, Duration::from_secs(20)) {
        return Err(ServiceError::Update(
            "Sabine rollback daemon did not become ready".to_string(),
        ));
    }
    Err(ServiceError::Update(format!(
        "Sabine {version} failed its startup check and was rolled back"
    )))
}

fn stop_failed_handoff_daemon() {
    #[cfg(target_os = "linux")]
    let _ = super::super::autostart::run_checked(std::process::Command::new("systemctl").args([
        "--user",
        "stop",
        "sabine.service",
    ]));
    #[cfg(target_os = "macos")]
    super::super::autostart::unload_macos_daemon();
    let _ = stop_daemon();
}

fn start_updated_daemon(service: &Path) -> ServiceResult<()> {
    if load_policy().login_autostart && install_login_autostart_with(service)? {
        return Ok(());
    }
    start_daemon_at(&crate::service_daemon_path(service))
}
