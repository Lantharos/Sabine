use std::{io, path::PathBuf, process::Stdio};

use sabine_runtime::background_command;
use sabine_service::StandaloneUpdates;

use crate::launch::bootstrap::{show_failure, update_accepted};
use crate::launch::executable::launch_executable;
use crate::window::config::SabineWindowConfig;

pub(crate) const APPLY_UPDATE_ARG: &str = "--sabine-apply-update";

fn updates(config: &SabineWindowConfig) -> Option<StandaloneUpdates> {
    StandaloneUpdates::new(
        config.app_id.as_deref()?,
        config.app_version.as_deref()?,
        config.app_update.as_ref()?,
    )
}

pub(super) fn keep_checking(config: &SabineWindowConfig) {
    let Some(updates) = updates(config) else {
        return;
    };
    if let Err(error) = std::thread::Builder::new()
        .name("sabine-updates".into())
        .spawn(move || updates.keep_checking())
    {
        sabine_runtime::report_error("updates", error);
    }
}

/// Asks to install an update downloaded earlier, and when the person agrees,
/// quits for a helper that installs it and starts the new version.
pub(super) fn offer_pending(config: &SabineWindowConfig) {
    let (Some(id), Some(updates)) = (config.app_id.as_deref(), updates(config)) else {
        return;
    };
    let Ok(Some(update)) = updates.pending() else {
        return;
    };
    if !update.ready_for_prompt() {
        return;
    }
    if !update_accepted(config, &update.version) {
        if let Err(error) = updates.defer(update) {
            sabine_runtime::report_error("updates", error);
        }
        return;
    }
    let (Ok(target), Ok(helper)) = (launch_executable(), helper_executable(id)) else {
        return;
    };
    let spawned = background_command(helper)
        .arg(APPLY_UPDATE_ARG)
        .arg(updates.pending_path())
        .arg(std::process::id().to_string())
        .arg(target)
        .stdin(Stdio::null())
        .spawn()
        .is_ok();
    if spawned {
        std::process::exit(0);
    }
}

/// The program that installs the update once the app has quit. Outside macOS
/// it runs from a copy, since Windows can't replace a program that is running
/// and an AppImage's files go away with the app.
fn helper_executable(id: &str) -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    if cfg!(target_os = "macos") {
        return Ok(executable);
    }
    let helper = std::env::temp_dir().join(format!("{id}-update{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(&executable, &helper)?;
    Ok(helper)
}

/// Runs as the helper: waits for the app to quit, installs its update and
/// starts it again, the new version or, when installing failed, the old one.
pub(crate) fn apply_update(args: &[String]) -> ! {
    let index = args
        .iter()
        .position(|arg| arg == APPLY_UPDATE_ARG)
        .expect("the helper runs with its argument");
    let [pending, pid, target] = &args[index + 1..] else {
        show_failure(
            "The update could not be installed",
            &"the update helper was started without its arguments",
        );
        std::process::exit(1);
    };
    if let Ok(pid) = pid.parse() {
        sabine_runtime::wait_for_exit(pid);
    }
    if let Err(error) = sabine_service::apply_standalone_update(pending.as_ref(), target.as_ref()) {
        show_failure("The update could not be installed", &error);
    }
    if let Err(error) = background_command(target).stdin(Stdio::null()).spawn() {
        show_failure("The app could not start again", &error);
    }
    std::process::exit(0);
}
