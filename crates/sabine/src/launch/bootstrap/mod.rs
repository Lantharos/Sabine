mod emergency;
mod icons;
pub(crate) mod installer;
mod standing;

use crate::launch::executable::launch_executable;
use crate::window::config::SabineWindowConfig;
use crate::{SabineError, SabineResult};
use sabine_runtime::{
    RuntimeConfig, RuntimeInfo, background_command, configure_background_command, resolve_runtime,
};
use sabine_service::{AppManifest, SabineService, prepare_machine_with_progress};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const CONFIRM_UPDATE_ARG: &str = "--sabine-confirm-update";

pub(crate) const NOTICE_ARG: &str = "--sabine-notice";

pub(crate) const BLOCKED_ARG: &str = "--sabine-blocked";

pub(crate) fn show_failure(title: &str, error: &dyn std::fmt::Display) {
    sabine_runtime::report_error("startup", error);
    let message = error.to_string();
    let shown = std::env::current_exe().ok().is_some_and(|executable| {
        background_command(executable)
            .args([NOTICE_ARG, title, &message])
            .status()
            .is_ok_and(|status| status.success())
    });
    if !shown {
        emergency::show(title, &message);
    }
}

pub(crate) const BOOTSTRAP_ARG: &str = "--sabine-bootstrap";

pub(crate) fn run_from_args(args: &[String]) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == CONFIRM_UPDATE_ARG) {
        let accepted =
            args.get(index + 1)
                .zip(args.get(index + 2))
                .is_some_and(|(title, version)| {
                    confirm_update(title, version).unwrap_or_else(|error| {
                        sabine_runtime::report_error("notice", &error);
                        emergency::show("Could not display the update prompt", &error);
                        false
                    })
                });
        std::process::exit(if accepted { 0 } else { 2 });
    }
    if let Some(index) = args.iter().position(|arg| arg == BLOCKED_ARG) {
        let remove = args
            .get(index + 1)
            .zip(args.get(index + 2))
            .is_some_and(|(title, body)| {
                standing::confirm_removal(title, body).unwrap_or_else(|error| {
                    sabine_runtime::report_error("notice", &error);
                    emergency::show(title, body);
                    false
                })
            });
        std::process::exit(if remove { 0 } else { 2 });
    }
    if let Some(index) = args.iter().position(|arg| arg == NOTICE_ARG) {
        if let (Some(title), Some(message)) = (args.get(index + 1), args.get(index + 2))
            && let Err(error) = sabine_dialog::show_notice(title, message, icons::sabine)
        {
            sabine_runtime::report_error("startup", error);
            emergency::show(title, message);
        }
        return true;
    }
    let Some(index) = args.iter().position(|arg| arg == BOOTSTRAP_ARG) else {
        return false;
    };
    let Some(path) = args.get(index + 1).map(PathBuf::from) else {
        eprintln!("missing Sabine bootstrap config");
        std::process::exit(1);
    };
    let BootstrapRequest { runtime, register } = match read_bootstrap(path) {
        Ok(request) => request,
        Err(error) => {
            show_failure("Sabine setup could not start", &error);
            std::process::exit(1);
        }
    };
    let result = sabine_dialog::run_progress_window(
        "Preparing Sabine",
        icons::sabine,
        move |state, proxy| {
            let result = prepare_machine_with_progress(runtime, register, |progress| {
                sabine_dialog::set_progress(&state, &proxy, progress.message, progress.fraction);
            });
            if let Err(error) = &result {
                sabine_runtime::report_error("setup", error);
            }
            sabine_dialog::finish(
                &state,
                &proxy,
                result.map(|_| ()).map_err(|error| error.to_string()),
            );
        },
    );
    match result {
        Ok(sabine_dialog::ProgressOutcome::Complete) => true,
        Ok(sabine_dialog::ProgressOutcome::Cancelled) => std::process::exit(2),
        Ok(sabine_dialog::ProgressOutcome::Failed(error)) => {
            show_failure("Sabine setup failed", &error);
            std::process::exit(3);
        }
        Err(error) => {
            show_failure("Sabine setup could not continue", &error);
            std::process::exit(1);
        }
    }
}

fn confirm_update(app_name: &str, version: &str) -> Result<bool, String> {
    sabine_dialog::confirm(sabine_dialog::Prompt {
        title: format!("Update {app_name}"),
        body: format!(
            "Version {version} is ready to install.\n\nThe app will close while the update is installed. You can choose Later to keep using this version."
        ),
        cancel: "Later".into(),
        confirm: "Install update".into(),
        cancel_by_default: false,
        icon: icons::app,
    })
}

/// Makes sure this machine has the Sabine service and a runtime, registering
/// the app, and returns the runtime to launch with.
pub(crate) fn prepare(config: &SabineWindowConfig) -> SabineResult<RuntimeInfo> {
    let register = app_manifest(config);
    let runtime = match resolve_runtime(&config.runtime) {
        Ok(runtime) => {
            adopt(config, &runtime, register)?;
            runtime
        }
        Err(_) => {
            run_bootstrap_install(config, register)?;
            resolve_runtime(&config.runtime)?
        }
    };
    relaunch_managed_update(config);
    offer_pending_update(config);
    standing::check_store_standing(config);
    Ok(runtime)
}

fn adopt(
    config: &SabineWindowConfig,
    runtime: &RuntimeInfo,
    register: Option<AppManifest>,
) -> SabineResult<()> {
    let report = match sabine_service::adopt(runtime, register) {
        Ok(report) => report,
        Err(error @ sabine_service::ServiceError::IncompatibleApp(_)) => {
            relaunch_managed_update(config);
            offer_pending_update(config);
            let message = error.to_string();
            return Err(SabineError::CreationFailed { message });
        }
        Err(error) => {
            return Err(SabineError::CreationFailed {
                message: format!("failed to register with Sabine service: {error}"),
            });
        }
    };
    if crate::launch::metrics::trace_enabled() {
        eprintln!(
            "sabine-service ready runtime={} daemon={} login_autostart={}",
            report.runtime_version, report.daemon_running, report.login_autostart
        );
    }
    Ok(())
}

fn relaunch_managed_update(config: &SabineWindowConfig) {
    if config.dev_mode() {
        return;
    }
    let Some(id) = config.app_id.as_deref() else {
        return;
    };
    let Ok(registered) = SabineService::default().app(id) else {
        return;
    };
    let Ok(current) = launch_executable() else {
        return;
    };
    let desired = registered.manifest.executable;
    let current = current.canonicalize().unwrap_or(current);
    let desired = desired.canonicalize().unwrap_or(desired);
    if current == desired || !desired.is_file() {
        return;
    }
    let mut command = background_command(desired);
    let launched = command
        .args(std::env::args_os().skip(1))
        .stdin(Stdio::null())
        .spawn()
        .is_ok();
    if launched {
        std::process::exit(0);
    }
}

fn offer_pending_update(config: &SabineWindowConfig) {
    if config.dev_mode() {
        return;
    }
    let Some(id) = config.app_id.as_deref() else {
        return;
    };
    let service = SabineService::default();
    let Ok(Some(update)) = service.pending_app_update(id) else {
        return;
    };
    if !update.ready_for_prompt() {
        return;
    }
    let accepted = std::env::current_exe().ok().is_some_and(|executable| {
        background_command(executable)
            .args([CONFIRM_UPDATE_ARG, &config.title, &update.version])
            .status()
            .is_ok_and(|status| status.success())
    });
    if !accepted {
        let _ = service.defer_pending_app_update(id);
        return;
    }
    let Ok(service_executable) = sabine_service::resolve_service_executable() else {
        return;
    };
    let Ok(executable) = launch_executable() else {
        return;
    };
    let mut command = background_command(service_executable);
    let spawned = command
        .arg("apply-update")
        .arg(id)
        .arg("--wait-pid")
        .arg(std::process::id().to_string())
        .arg("--relaunch")
        .arg(executable)
        .stdin(Stdio::null())
        .spawn()
        .is_ok();
    if spawned {
        std::process::exit(0);
    }
}

fn run_bootstrap_install(
    config: &SabineWindowConfig,
    register: Option<AppManifest>,
) -> SabineResult<()> {
    let config_path = bootstrap_config_path();
    let request = BootstrapRequest {
        runtime: config.runtime.clone(),
        register,
    };
    write_bootstrap(&config_path, &request).map_err(|error| SabineError::CreationFailed {
        message: format!("failed to prepare Sabine bootstrap: {error}"),
    })?;
    let executable = std::env::current_exe().map_err(|error| SabineError::CreationFailed {
        message: format!("failed to locate app executable: {error}"),
    })?;
    let mut command = Command::new(executable);
    configure_background_command(&mut command);
    let status = command
        .arg(BOOTSTRAP_ARG)
        .arg(&config_path)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| SabineError::CreationFailed {
            message: format!("failed to launch Sabine bootstrap: {error}"),
        })?;
    let _ = std::fs::remove_file(config_path);
    match status.code() {
        Some(0) => Ok(()),
        Some(2) => Err(SabineError::SetupCancelled),
        Some(3) => Err(SabineError::SetupFailed),
        _ => Err(SabineError::CreationFailed {
            message: format!(
                "Sabine setup stopped unexpectedly ({status}); details: {}",
                sabine_runtime::diagnostic_path("setup").display()
            ),
        }),
    }
}

pub(crate) fn app_manifest(config: &SabineWindowConfig) -> Option<AppManifest> {
    let id = config.app_id.as_ref()?;
    let executable = launch_executable().ok()?;
    Some(AppManifest {
        id: id.clone(),
        name: config.title.clone(),
        version: config
            .app_version
            .clone()
            .unwrap_or_else(|| "0.0.0".to_string()),
        executable,
        args: Vec::new(),
        update: config.app_update.clone(),
        sabine: sabine_service::SabineVersion::current(),
    })
}

/// What the setup process prepares, handed to it in a file.
#[derive(Deserialize, Serialize)]
struct BootstrapRequest {
    runtime: RuntimeConfig,
    register: Option<AppManifest>,
}

fn write_bootstrap(path: &Path, request: &BootstrapRequest) -> std::io::Result<()> {
    std::fs::write(path, serde_json::to_vec(request)?)
}

fn read_bootstrap(path: PathBuf) -> Result<BootstrapRequest, String> {
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(path);
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid Sabine setup request: {error}"))
}

fn bootstrap_config_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "sabine-bootstrap-{}-{nonce}.json",
        std::process::id()
    ))
}
