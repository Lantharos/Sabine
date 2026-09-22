use sabine_service::{AppUpdateStatus, SabineService};
use std::{path::Path, process::ExitCode};

pub fn run(target: Option<String>, all: bool, force: bool) -> Result<ExitCode, String> {
    if all && target.is_some() {
        return Err("use either an app/component target or --all".into());
    }
    match target.as_deref() {
        None => {
            update_sabine(force)?;
            update_cef()?;
        }
        Some("sabine" | "system") => update_sabine(force)?,
        Some("cef" | "runtime") => update_cef()?,
        Some(target) => {
            update_app(target, force)?;
            return Ok(ExitCode::SUCCESS);
        }
    }
    if all {
        for app in SabineService::default()
            .apps()
            .map_err(|error| error.to_string())?
        {
            update_app(&app.manifest.id, force)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn update_sabine(force: bool) -> Result<(), String> {
    println!("Checking Sabine components…");
    let mut last = String::new();
    let report =
        sabine_service::update_components(force, sabine_service::SABINE_VERSION, |progress| {
            if progress.message != last {
                println!("{}", progress.message);
                last = progress.message;
            }
        })
        .map_err(|error| error.to_string())?;
    if report.deferred {
        println!(
            "Sabine {} is waiting for its soak period; use --force to install it now",
            report.version
        );
        return Ok(());
    }
    if let Some(cli) = &report.cli {
        self_replace::self_replace(cli)
            .map_err(|error| format!("could not update the CLI: {error}"))?;
        std::fs::remove_file(cli).map_err(|error| error.to_string())?;
        println!("Updated Sabine CLI to {}", report.version);
    }
    if report.system_updated {
        println!(
            "Updated Sabine service, daemon and host to {}",
            report.version
        );
    } else {
        println!("Sabine service and host are current");
    }
    Ok(())
}

fn update_cef() -> Result<(), String> {
    println!("Checking CEF and validating Chromium startup…");
    sabine_service::ensure_service_executable(|progress| println!("{}", progress.message))
        .map_err(|error| error.to_string())?;
    let runtime = SabineService::default()
        .update_runtime_with_progress(|progress| println!("{}", progress.message))
        .map_err(|error| error.to_string())?;
    println!("CEF {} is ready", runtime.version);
    Ok(())
}

fn update_app(target: &str, force: bool) -> Result<(), String> {
    if Path::new(target).exists() || crate::install::source::read_registered_app(target).is_ok() {
        crate::install::source::update(crate::install::source::UpdateOptions {
            target: Some(target.into()),
            all: false,
        })?;
        return Ok(());
    }
    let service = SabineService::default();
    match service
        .update_app_with_soak(target, !force)
        .map_err(|error| error.to_string())?
    {
        AppUpdateStatus::Current => println!("{target} is current"),
        AppUpdateStatus::Installed { version } => println!("Updated {target} to {version}"),
        AppUpdateStatus::Deferred { version } => println!(
            "{target} {version} is waiting for its soak period; use --force to install it now"
        ),
        AppUpdateStatus::StoreManaged => println!("Update {target} through its app store"),
        AppUpdateStatus::RequiresSystem { sabine, .. } => {
            return Err(format!(
                "{target} requires Sabine {}; run sabine update first",
                sabine.label()
            ));
        }
        AppUpdateStatus::PendingApproval(pending) => println!(
            "{} {} is downloaded; open the app to approve its package installer",
            target, pending.version
        ),
    }
    Ok(())
}
