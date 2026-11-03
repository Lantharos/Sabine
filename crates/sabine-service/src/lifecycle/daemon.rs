use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use crate::{
    SabineService, ServiceError, ServiceResult, ensure_service_executable,
    install::{
        StagedSystemUpdate, daemon_paths_for_version, repair_system_installation,
        rollback_system_update, service_path_for_version, stage_required_system_update,
        stage_system_update,
    },
    service_daemon_path,
};
use sabine_runtime::{configure_background_command, process_alive, sabine_data_dir};

use super::{autostart::install_login_autostart_with, load_policy};

#[cfg(target_os = "linux")]
use super::autostart::{retire_shadowing_user_unit, supervised_daemon_matches};

mod handoff;
mod ownership;
mod process;

pub use handoff::complete_system_update;
use ownership::{DAEMON_STATE_FILE, claim_daemon, daemon_state};
use process::{process_executable, same_executable, terminate_process};

pub fn ensure_daemon_running() -> ServiceResult<bool> {
    let service = ensure_service_executable(|_| {})?;
    let expected_version = service_version(&service).ok_or_else(|| {
        ServiceError::Update(format!(
            "Sabine service has no version directory: {}",
            service.display()
        ))
    })?;
    let daemon = service_daemon_path(&service);
    let login_autostart = load_policy().login_autostart;
    let matching_daemon = daemon_state()
        .is_some_and(|state| state.version == expected_version && process_alive(state.pid));
    if matching_daemon {
        #[cfg(target_os = "linux")]
        if login_autostart && !supervised_daemon_matches(&daemon) {
            stop_daemon()?;
            if install_login_autostart_with(&service).unwrap_or(false)
                && wait_for_daemon_version(&expected_version, Duration::from_secs(2))
            {
                return Ok(true);
            }
        } else {
            return Ok(true);
        }
        #[cfg(not(target_os = "linux"))]
        return Ok(true);
    }
    if login_autostart && install_login_autostart_with(&service).unwrap_or(false) {
        if wait_for_daemon_version(&expected_version, Duration::from_secs(2)) {
            return Ok(true);
        }
        stop_daemon()?;
        if install_login_autostart_with(&service).unwrap_or(false)
            && wait_for_daemon_version(&expected_version, Duration::from_secs(2))
        {
            return Ok(true);
        }
    }
    stop_daemon()?;
    start_daemon_at(&daemon)?;
    if wait_for_daemon_version(&expected_version, Duration::from_secs(2)) {
        return Ok(true);
    }
    stop_daemon()?;
    if let Some(previous) = rollback_system_update(&expected_version)? {
        let previous_version = service_version(&previous).ok_or_else(|| {
            ServiceError::Update("rollback Sabine service has no version".to_string())
        })?;
        if login_autostart
            && install_login_autostart_with(&previous).unwrap_or(false)
            && wait_for_daemon_version(&previous_version, Duration::from_secs(2))
        {
            return Ok(true);
        }
        start_daemon_at(&service_daemon_path(&previous))?;
        if wait_for_daemon_version(&previous_version, Duration::from_secs(20)) {
            return Ok(true);
        }
        stop_daemon()?;
    }
    let repaired = repair_system_installation()?;
    let repaired_version = service_version(&repaired).ok_or_else(|| {
        ServiceError::Update("repaired Sabine service has no version".to_string())
    })?;
    let repaired_daemon = service_daemon_path(&repaired);
    if login_autostart
        && install_login_autostart_with(&repaired).unwrap_or(false)
        && wait_for_daemon_version(&repaired_version, Duration::from_secs(2))
    {
        return Ok(true);
    }
    start_daemon_at(&repaired_daemon)?;
    if wait_for_daemon_version(&repaired_version, Duration::from_secs(20)) {
        return Ok(true);
    }
    Err(ServiceError::Update(
        "Sabine service did not become ready after a clean repair".to_string(),
    ))
}

pub fn running_daemon_version() -> Option<String> {
    let state = daemon_state().filter(|state| process_alive(state.pid))?;
    let actual = process_executable(state.pid)?;
    let expected = service_daemon_path(&service_path_for_version(&state.version));
    (actual.file_name() == expected.file_name()).then_some(state.version)
}

pub fn run_daemon() -> ServiceResult<()> {
    #[cfg(target_os = "linux")]
    {
        if let Err(error) = retire_shadowing_user_unit() {
            sabine_runtime::report_error("maintenance", error);
        }
        hand_over_to_newer_installation();
    }
    let Some(owner) = claim_daemon()? else {
        return Ok(());
    };
    #[cfg(target_os = "linux")]
    show_sabine_in_desktop();
    schedule_installation_finalization(owner.pid);
    let service = SabineService::default();
    let mut next_maintenance = Instant::now();
    loop {
        if Instant::now() >= next_maintenance {
            if run_maintenance(&service) {
                return Ok(());
            }
            next_maintenance = Instant::now() + crate::default_maintenance_interval();
        }
        if let Err(error) = service.refresh_store_revocations() {
            sabine_runtime::report_error("schelf", error);
        }
        std::thread::sleep(
            crate::schelf::REVOCATIONS_REFRESH
                .min(next_maintenance.saturating_duration_since(Instant::now())),
        );
    }
}

/// Updates Sabine and the apps and clears out removed apps, and says whether
/// the daemon handed over to a newer Sabine and should exit.
fn run_maintenance(service: &SabineService) -> bool {
    if hand_over(stage_system_update()) {
        return true;
    }
    match service.maintain() {
        Ok(report) => {
            for error in &report.update_failures {
                sabine_runtime::report_error("maintenance", error);
            }
            if let Some(required) = report.required_system_update
                && hand_over(stage_required_system_update(required))
            {
                return true;
            }
        }
        Err(error) => sabine_runtime::report_error("maintenance", error),
    }
    match service.clean_up_apps() {
        Ok(cleanup) => {
            for id in &cleanup.removed {
                let _ = sabine_runtime::record_diagnostic(
                    "maintenance",
                    &format!("removed {id}, which is no longer installed"),
                );
            }
            for failure in &cleanup.failures {
                sabine_runtime::report_error("maintenance", failure);
            }
        }
        Err(error) => sabine_runtime::report_error("maintenance", error),
    }
    false
}

/// Whether this daemon handed over to the update `staged`, and should exit.
/// A failed update is reported, and the daemon keeps running the current one.
fn hand_over(staged: ServiceResult<Option<StagedSystemUpdate>>) -> bool {
    let result = staged.and_then(|update| match update {
        Some(update) => handoff::begin_system_handoff(&update).map(|()| true),
        None => Ok(false),
    });
    result.unwrap_or_else(|error| {
        sabine_runtime::report_error("maintenance", error);
        false
    })
}

/// Becomes the daemon of the newest installation, so a daemon the system
/// starts serves a user whose own Sabine is newer.
#[cfg(target_os = "linux")]
fn hand_over_to_newer_installation() {
    use std::os::unix::process::CommandExt;
    let Some(service) = crate::find_service_executable() else {
        return;
    };
    let newer = service_version(&service)
        .is_some_and(|version| crate::types::version_is_newer(&version, crate::SABINE_VERSION));
    if newer {
        let error = Command::new(service_daemon_path(&service)).exec();
        sabine_runtime::report_error("maintenance", error);
    }
}

#[cfg(target_os = "linux")]
fn show_sabine_in_desktop() {
    let result = std::env::current_exe().and_then(|daemon| {
        let daemon = crate::install::stable_daemon_path(&daemon).unwrap_or(daemon);
        crate::identity::install_for_user(&daemon)
    });
    if let Err(error) = result {
        sabine_runtime::report_error("maintenance", error);
    }
}

fn schedule_installation_finalization(pid: u32) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        let healthy = daemon_state().is_some_and(|state| {
            state.pid == pid && state.version == crate::SABINE_VERSION && process_alive(state.pid)
        });
        if healthy {
            crate::install::mark_system_update_healthy(crate::SABINE_VERSION);
        }
    });
}

fn start_daemon_at(executable: &Path) -> ServiceResult<()> {
    let _ = fs::create_dir_all(sabine_data_dir());
    let mut command = Command::new(executable);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_background_command(&mut command);
    command.spawn().map(|_| ()).map_err(|error| {
        ServiceError::Update(format!(
            "failed to launch {}: {error}",
            executable.display()
        ))
    })
}

pub(crate) fn service_version(service: &Path) -> Option<String> {
    let parent = service.parent()?;
    if parent.parent()?.file_name()?.to_str()? == "versions" {
        return parent.file_name()?.to_str().map(ToString::to_string);
    }
    Some(crate::SABINE_VERSION.to_string())
}

/// Stops the daemon recorded as running, when it is a Sabine daemon.
pub(crate) fn stop_daemon() -> ServiceResult<()> {
    let Some(state) = daemon_state().filter(|state| process_alive(state.pid)) else {
        return Ok(());
    };
    let Some(actual) = process_executable(state.pid) else {
        return Err(ServiceError::Update(format!(
            "could not verify stale Sabine service {} before stopping it",
            state.pid
        )));
    };
    let state_file = sabine_data_dir().join(DAEMON_STATE_FILE);
    if !daemon_paths_for_version(&state.version)
        .iter()
        .any(|expected| same_executable(&actual, expected))
    {
        let _ = fs::remove_file(state_file);
        return Ok(());
    }
    terminate_process(state.pid)?;
    for _ in 0..40 {
        if !process_alive(state.pid) {
            let _ = fs::remove_file(state_file);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(ServiceError::Update(format!(
        "stale Sabine service {} did not stop",
        state.pid
    )))
}

fn wait_for_daemon_version(version: &str, timeout: Duration) -> bool {
    let started = std::time::Instant::now();
    let mut matching_since = None;
    while started.elapsed() < timeout {
        if daemon_state().is_some_and(|state| state.version == version && process_alive(state.pid))
        {
            matching_since.get_or_insert_with(std::time::Instant::now);
            if matching_since.is_some_and(|seen| seen.elapsed() >= Duration::from_secs(1)) {
                return true;
            }
        } else {
            matching_since = None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

pub fn resolve_service_executable() -> ServiceResult<PathBuf> {
    crate::find_service_executable().ok_or_else(|| {
        ServiceError::Update(
            "sabine-service executable not found; it will be downloaded on first launch, or set SABINE_SERVICE_PATH / SABINE_RELEASE_MANIFEST_URL".to_string(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::service_version;
    use std::path::Path;

    #[test]
    fn service_version_distinguishes_managed_and_adjacent_binaries() {
        assert_eq!(
            service_version(Path::new("Sabine/bin/versions/0.1.14/sabine-service")),
            Some("0.1.14".to_string())
        );
        assert_eq!(
            service_version(Path::new("target/debug/sabine-service")),
            Some(crate::SABINE_VERSION.to_string())
        );
    }
}
