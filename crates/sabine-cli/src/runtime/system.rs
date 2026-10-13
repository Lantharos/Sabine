use std::path::Path;

use sabine_runtime::{
    RuntimeConfig, path_in_root, resolve_runtime, system_dir, system_runtime_path,
};

/// Installs the Sabine service, CEF host and CEF runtime for every user of the
/// machine, or stages them under `root`. The pieces come from this machine's
/// own Sabine: a service next to the CLI or already installed, a host built
/// for the runtime, and the newest runtime, downloaded when there is none.
pub(super) fn install(root: Option<&Path>) -> Result<(), String> {
    let runtime = match resolve_runtime(&RuntimeConfig::default()) {
        Ok(runtime) => runtime,
        Err(_) => super::install_default_runtime(&RuntimeConfig::default())?,
    };
    let host = sabine_host::ensure_host(runtime.location.path())
        .map_err(|error| format!("failed to prepare the Sabine CEF host: {error}"))?;
    let service = match sabine_service::find_service_executable() {
        Some(service) => service,
        None => sabine_service::ensure_service_executable(|progress| {
            eprintln!("sabine: {}", progress.message)
        })
        .map_err(|error| format!("failed to prepare the Sabine service: {error}"))?,
    };
    let system = path_in_root(root, &system_dir());
    let version = sabine_service::install_system_service(&service, &host, &system)
        .map_err(|error| format!("failed to install the Sabine service: {error}"))?;
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
