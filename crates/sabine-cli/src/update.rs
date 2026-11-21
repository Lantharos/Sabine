use sabine_service::{AppUpdateStatus, SabineService};
use std::{path::Path, process::ExitCode};

use crate::ui::{self, Step};

pub fn run(target: Option<String>, all: bool, force: bool) -> Result<ExitCode, String> {
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
    let step = Step::start("Updating Sabine");
    let report = sabine_service::update_components(force, |progress| {
        step.progress(&progress.message, progress.fraction)
    })
    .map_err(|error| error.to_string())?;
    if report.deferred {
        step.done_with(&format!("{} is still being tested", report.version));
        ui::hint("Add --force to install it now");
        return Ok(());
    }
    match (&report.system, &report.held) {
        (Some(version), _) => step.done_with(&format!("now {version}")),
        (None, None) => step.done_with("up to date"),
        (None, Some(held)) => {
            step.done_with(&format!("{} is waiting", held.version));
            let apps = held
                .apps
                .iter()
                .map(|app| app.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            ui::hint(&format!(
                "It would no longer run {apps}; update or remove them to install it"
            ));
        }
    }
    if let Some(path) = crate::handover::system_owned_entry() {
        ui::hint(&format!(
            "{} updates with the system, so Sabine leaves it alone",
            path.display()
        ));
    }
    Ok(())
}

fn update_cef() -> Result<(), String> {
    let step = Step::start("Updating CEF");
    sabine_service::ensure_service_executable(|progress| {
        step.progress(&progress.message, progress.fraction)
    })
    .map_err(|error| error.to_string())?;
    let runtime = SabineService::default()
        .update_runtime_with_progress(|progress| {
            step.progress(&progress.message, progress.fraction)
        })
        .map_err(|error| error.to_string())?;
    step.done_with(&runtime.version);
    Ok(())
}

fn update_app(target: &str, force: bool) -> Result<(), String> {
    if Path::new(target).is_dir() {
        let id = crate::install::project_install_id(Path::new(target))?;
        if let Some(mut install) = crate::install::bundle_install(&id)? {
            install.source = std::fs::canonicalize(target).map_err(|error| error.to_string())?;
            return crate::install::rebuild_bundle(&id, install);
        }
        return crate::install::source::update(target).map(drop);
    }
    if crate::install::source::read_registered_app(target).is_ok() {
        return crate::install::source::update(target).map(drop);
    }
    let service = SabineService::default();
    if service
        .app(target)
        .map_err(|error| error.to_string())?
        .manifest
        .update
        .is_none()
    {
        if let Some(install) = crate::install::bundle_install(target)? {
            return crate::install::rebuild_bundle(target, install);
        }
        ui::result(target, "has nowhere to update from");
        return Ok(());
    }
    let step = Step::start(format!("Updating {target}"));
    match service
        .update_app_with_soak(target, !force)
        .map_err(|error| error.to_string())?
    {
        AppUpdateStatus::Current => step.done_with("up to date"),
        AppUpdateStatus::Installed { version } => step.done_with(&format!("now {version}")),
        AppUpdateStatus::Deferred { version } => {
            step.done_with(&format!("{version} is still being tested"));
            ui::hint("Add --force to install it now");
        }
        AppUpdateStatus::StoreManaged => {
            step.done_with("updates through its app store");
        }
        AppUpdateStatus::Withdrawn => {
            step.done_with("no longer gets updates from Schelf");
        }
        AppUpdateStatus::RequiresSystem { sabine, .. } => {
            return Err(format!(
                "{target} needs Sabine {}; run `sabine update` first",
                sabine.label()
            ));
        }
        AppUpdateStatus::PendingApproval(pending) => {
            step.done_with(&format!("{} is downloaded", pending.version));
            ui::hint("Open the app to approve its installer");
        }
    }
    Ok(())
}
