use std::path::Path;

use sabine_runtime::{path_in_root, system_dir, system_runtime_path};

/// Installs the Sabine service, this CLI, the CEF host and the CEF runtime for
/// every user of the machine, or stages them under `root`. The pieces come
/// from this machine's own Sabine: a service next to the CLI or already
/// installed, a host built for the runtime, and the newest runtime, downloaded
/// when there is none.
pub(super) fn install(root: Option<&Path>) -> Result<(), String> {
    let runtime = super::ensure_runtime()?;
    let host = sabine_host::ensure_host(runtime.location.path())
        .map_err(|error| format!("failed to prepare the Sabine CEF host: {error}"))?;
    let service = match sabine_service::find_service_executable() {
        Some(service) => service,
        None => {
            let mut log = sabine_runtime::ProgressLog::default();
            sabine_service::ensure_service_executable(|progress| {
                if let Some(line) = log.line(&progress.message, progress.fraction) {
                    eprintln!("sabine: {line}");
                }
            })
            .map_err(|error| format!("failed to prepare the Sabine service: {error}"))?
        }
    };
    let system = path_in_root(root, &system_dir());
    let cli = std::env::current_exe().map_err(|error| error.to_string())?;
    let version = sabine_service::install_system_service(&service, &host, &cli, &system)
        .map_err(|error| format!("failed to install the Sabine service: {error}"))?;
    #[cfg(target_os = "linux")]
    sabine_service::install_system_unit(root)
        .map_err(|error| format!("failed to install the Sabine user unit: {error}"))?;
    sabine_runtime::install_system_runtime(
        runtime.location.path(),
        &path_in_root(root, &system_runtime_path()),
    )
    .map_err(|error| format!("failed to install the CEF runtime: {error}"))?;
    println!(
        "Installed Sabine {version} and CEF runtime {} for every user in {}.",
        runtime.version,
        system.display()
    );
    Ok(())
}
