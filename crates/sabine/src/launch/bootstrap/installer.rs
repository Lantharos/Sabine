mod progress;

use crate::window::config::SabineWindowConfig;
use progress::SetupProgress;
use sabine_service::{SabineService, prepare_machine_with_progress};
use std::ffi::{OsStr, OsString};
use std::path::Path;

pub(crate) const INSTALL_ARG: &str = "--sabine-install";
pub(crate) const UNINSTALL_ARG: &str = "--sabine-uninstall";
const PREPARE_SHARE: f32 = 0.9;

pub(crate) fn requested(args: &[OsString]) -> bool {
    args.iter()
        .any(|arg| arg == INSTALL_ARG || arg == UNINSTALL_ARG)
}

pub(crate) fn run(config: &SabineWindowConfig, args: &[OsString]) -> ! {
    let cancelled =
        || option(args, "--sabine-install-cancel").is_some_and(|path| Path::new(path).exists());
    let uninstalling = args.iter().any(|arg| arg == UNINSTALL_ARG);
    let result = match (config.standalone(), uninstalling) {
        (true, true) => Ok(()),
        (true, false) => check_standalone(config),
        (false, true) => unregister(config),
        (false, false) => prepare(
            config,
            option(args, "--sabine-install-to").map(Path::new),
            cancelled,
            SetupProgress::from_args(args),
        ),
    };
    match result {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            if cancelled() {
                println!("Installation cancelled.");
                std::process::exit(1602);
            }
            sabine_runtime::report_error("installer", &error);
            println!("{error}");
            println!(
                "Details: {}",
                sabine_runtime::diagnostic_path("installer").display()
            );
            std::process::exit(1);
        }
    }
}

fn option<'a>(args: &'a [OsString], name: &str) -> Option<&'a OsStr> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(OsString::as_os_str)
}

fn prepare(
    config: &SabineWindowConfig,
    destination: Option<&Path>,
    cancelled: impl Fn() -> bool,
    mut progress: SetupProgress,
) -> Result<(), String> {
    config.validate().map_err(|error| error.to_string())?;
    let manifest = super::app_manifest(config).ok_or("The app has no installation identity")?;
    let report = prepare_machine_with_progress(config.runtime.clone(), None, |update| {
        if cancelled() {
            println!("Installation cancelled.");
            std::process::exit(1602);
        }
        let fraction = update.fraction.map(|fraction| fraction * PREPARE_SHARE);
        progress.report(&update.message, fraction);
    })
    .map_err(|error| error.to_string())?;
    if !report.daemon_running {
        return Err(
            "The Sabine background service could not start. Retry setup to repair it.".into(),
        );
    }
    let runtime =
        sabine_runtime::resolve_runtime(&config.runtime).map_err(|error| error.to_string())?;
    let host = sabine_host::available_host(runtime.location.path())
        .ok_or("The Sabine native host is missing. Retry setup to repair it.")?;
    sabine_runtime::prepare_runtime_assets(runtime.location.path())
        .map_err(|error| error.to_string())?;
    sabine_host::validate_host_protocol(&host, runtime.location.path())?;
    if let Some(destination) = destination {
        progress.report("Installing application files", Some(PREPARE_SHARE));
        let source = manifest
            .executable
            .parent()
            .ok_or("The installer payload has no directory")?
            .to_path_buf();
        SabineService::default()
            .install_app_payload(&source, destination, manifest, cancelled)
            .map_err(|error| error.to_string())?;
    } else {
        SabineService::default()
            .register(manifest)
            .map_err(|error| error.to_string())?;
    }
    progress.report("Installation is ready", Some(1.0));
    Ok(())
}

/// A standalone app's setup installs its files itself and registers nothing:
/// the app only checks that the Chromium and host it ships with start.
fn check_standalone(config: &SabineWindowConfig) -> Result<(), String> {
    let runtime = super::standalone::bundled_runtime(config).map_err(|error| error.to_string())?;
    sabine_runtime::prepare_runtime_assets(runtime.location.path())
        .map_err(|error| error.to_string())?;
    let host = sabine_host::bundled_host()
        .ok_or("The Sabine host this app ships with is missing. Retry setup to repair it.")?;
    sabine_host::validate_host_protocol(&host, runtime.location.path())
}

fn unregister(config: &SabineWindowConfig) -> Result<(), String> {
    let id = config
        .app_id
        .as_deref()
        .ok_or("The app has no installation identity")?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let install = executable
        .parent()
        .ok_or("The app executable has no directory")?;
    SabineService::default()
        .forget_app(id, install)
        .map_err(|error| error.to_string())?;
    println!("Application registration removed.");
    Ok(())
}
