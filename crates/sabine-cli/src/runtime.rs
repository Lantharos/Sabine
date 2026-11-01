use std::process::ExitCode;

use sabine_runtime::{
    ProgressLog, RuntimeConfig, RuntimeInfo, RuntimeInstallProgress, detect_runtime,
    prune_user_runtimes, remove_user_runtime_version, resolve_runtime,
};
use sabine_service::SabineService;

#[cfg(target_os = "linux")]
mod sandbox;
mod system;

pub enum RuntimeCommand {
    Prepare,
    #[cfg(target_os = "linux")]
    SandboxProfile,
    List {
        json: bool,
    },
    Install,
    InstallSystemWide {
        root: Option<std::path::PathBuf>,
    },
    Remove {
        version: String,
    },
    Prune {
        keep: usize,
    },
    Doctor {
        json: bool,
    },
}

pub fn run_runtime(command: RuntimeCommand) -> ExitCode {
    let result = match command {
        #[cfg(target_os = "linux")]
        RuntimeCommand::SandboxProfile => sandbox::profile().map(|profile| print!("{profile}")),
        RuntimeCommand::Prepare => {
            ensure_runtime_ready().map(|host| println!("{}", host.display()))
        }
        RuntimeCommand::List { json } => return list_runtimes(json),
        RuntimeCommand::Install => install_runtime(),
        RuntimeCommand::InstallSystemWide { root } => system::install(root.as_deref()),
        RuntimeCommand::Remove { version } => return remove_runtime(&version),
        RuntimeCommand::Prune { keep } => return prune_runtime(keep),
        RuntimeCommand::Doctor { json } => return doctor_runtime(json),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

/// The shared CEF runtime, downloaded when this machine has none.
pub(crate) fn ensure_runtime() -> Result<RuntimeInfo, String> {
    if resolve_runtime(&RuntimeConfig::default()).is_err() {
        eprintln!("sabine: no CEF runtime found; installing the shared runtime…");
    }
    SabineService::default()
        .ensure_runtime_with_progress(progress_printer())
        .map_err(|error| {
            format!(
                "failed to prepare the Sabine runtime: {error}\ninstall it with `sabine runtime install`"
            )
        })
}

/// The CEF host for the shared runtime, which is installed first when missing.
pub fn ensure_runtime_ready() -> Result<std::path::PathBuf, String> {
    let runtime = ensure_runtime()?;
    sabine_host::ensure_host(runtime.location.path())
        .map_err(|error| format!("failed to prepare the Sabine CEF host: {error}"))
}

fn progress_printer() -> impl FnMut(RuntimeInstallProgress) {
    let mut log = ProgressLog::default();
    move |progress| {
        if let Some(line) = log.line(&progress.message, progress.fraction) {
            eprintln!("sabine: {line}");
        }
    }
}

fn list_runtimes(json: bool) -> ExitCode {
    let config = RuntimeConfig::default();
    let runtimes = detect_runtime(&config);
    if json {
        let entries = runtimes
            .iter()
            .map(|r| {
                let location_type = match &r.location {
                    sabine_runtime::RuntimeLocation::System(_) => "system",
                    sabine_runtime::RuntimeLocation::UserLocal(_) => "user",
                    sabine_runtime::RuntimeLocation::Bundled(_) => "bundled",
                };
                serde_json::json!({
                    "version": r.version,
                    "location_type": location_type,
                    "path": r.location.path(),
                    "verified": r.verified,
                })
            })
            .collect::<Vec<_>>();

        println!("{}", serde_json::json!({ "runtimes": entries }));
    } else if runtimes.is_empty() {
        println!("No CEF runtimes found.");
        println!("Run `sabine runtime install` to install the shared runtime.");
    } else {
        println!("CEF runtimes:");
        for runtime in &runtimes {
            let location_type = match &runtime.location {
                sabine_runtime::RuntimeLocation::System(_) => "system",
                sabine_runtime::RuntimeLocation::UserLocal(_) => "user",
                sabine_runtime::RuntimeLocation::Bundled(_) => "bundled",
            };
            println!(
                "  {} {} {}{}",
                runtime.version,
                location_type,
                runtime.location.path().display(),
                if runtime.verified {
                    ""
                } else {
                    " (quarantined)"
                }
            );
        }
    }

    ExitCode::SUCCESS
}

/// Installs the newest CEF runtime and checks that Chromium starts on it.
fn install_runtime() -> Result<(), String> {
    let previous = resolve_runtime(&RuntimeConfig::default())
        .ok()
        .map(|runtime| runtime.version);
    let runtime = SabineService::default()
        .update_runtime_with_progress(progress_printer())
        .map_err(|error| format!("failed to install runtime: {error}"))?;
    if previous.as_deref() == Some(runtime.version.as_str()) {
        println!(
            "Latest CEF runtime {} is already installed at {}.",
            runtime.version,
            runtime.location.path().display()
        );
    } else {
        println!(
            "Installed CEF runtime {} at {}.",
            runtime.version,
            runtime.location.path().display()
        );
    }
    Ok(())
}

fn remove_runtime(version: &str) -> ExitCode {
    match remove_user_runtime_version(version) {
        Ok(true) => {
            println!("Removed CEF runtime {version}.");
            ExitCode::SUCCESS
        }
        Ok(false) => {
            eprintln!("No user-local CEF runtime {version} found.");
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("failed to remove CEF runtime {version}: {error}");
            ExitCode::from(1)
        }
    }
}

fn prune_runtime(keep: usize) -> ExitCode {
    match prune_user_runtimes(keep) {
        Ok(0) => {
            println!("No stale CEF runtimes found.");
            ExitCode::SUCCESS
        }
        Ok(removed) => {
            println!("Removed {removed} stale CEF runtime(s).");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("failed to prune CEF runtimes: {error}");
            ExitCode::from(1)
        }
    }
}

fn doctor_runtime(json: bool) -> ExitCode {
    let config = RuntimeConfig::default();
    let runtimes = detect_runtime(&config);
    let resolved = resolve_runtime(&config).ok();
    let has_compatible = resolved.is_some();
    let host = resolved
        .as_ref()
        .and_then(|runtime| sabine_host::available_host(runtime.location.path()));
    let probe_error = host.as_ref().and_then(|host| {
        sabine_host::smoke_test_runtime(host, resolved.as_ref()?.location.path()).err()
    });
    let host_ready = host.is_some() && probe_error.is_none();
    let status = if has_compatible && host_ready {
        "ok"
    } else if has_compatible && host.is_none() {
        "host-missing"
    } else if has_compatible {
        "host-unhealthy"
    } else if runtimes.iter().any(|runtime| !runtime.verified) {
        "quarantined"
    } else if runtimes.is_empty() {
        "missing"
    } else {
        "outdated"
    };

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": status,
                "host_ready": host_ready,
                "probe_error": probe_error,
                "runtimes": runtimes.iter().map(|runtime| serde_json::json!({
                    "version": runtime.version,
                    "location": runtime.location.path(),
                    "verified": runtime.verified,
                })).collect::<Vec<_>>(),
            })
        );
    } else {
        match status {
            "ok" => println!("CEF runtime: ok"),
            "host-missing" => println!("CEF runtime: present, Sabine host is missing"),
            "host-unhealthy" => {
                println!("CEF runtime: present, Sabine host failed its launch probe");
                if let Some(error) = &probe_error {
                    println!("  {error}");
                }
            }
            "missing" => {
                println!("CEF runtime: not found");
                println!("  Install with: sabine runtime install");
            }
            "outdated" => {
                println!(
                    "CEF runtime: outdated (found versions below minimum {})",
                    sabine_runtime::MIN_CEF_MAJOR
                );
                println!("  Update with: sabine runtime install");
            }
            "quarantined" => {
                println!("CEF runtime: quarantined after a failed health probe");
                for runtime in runtimes.iter().filter(|runtime| !runtime.verified) {
                    let marker = runtime.location.path().join(".sabine-unusable");
                    if let Ok(reason) = std::fs::read_to_string(marker) {
                        println!("  {}: {}", runtime.location.path().display(), reason.trim());
                    }
                }
            }
            _ => {}
        }
        if has_compatible && status != "host-unhealthy" {
            println!(
                "Sabine host: {}",
                if host_ready {
                    "ready"
                } else {
                    "missing; run `sabine runtime prepare`"
                }
            );
        }
    }

    if has_compatible && host_ready {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
