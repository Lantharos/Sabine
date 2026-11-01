use std::{fs, path::Path, process::Command};

use super::run_checked;
use crate::{ServiceError, ServiceResult};

const LABEL: &str = "net.lantharos.sabine";
const HELPER_LABEL: &str = "net.lantharos.sabine.update";

pub(super) fn install(_executable: &Path, daemon: &Path) -> ServiceResult<bool> {
    let path = agent_path()?;
    fs::create_dir_all(path.parent().expect("LaunchAgents has a parent"))?;
    fs::write(
        &path,
        property_list(
            LABEL,
            &[daemon.to_string_lossy().into_owned()],
            "<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>",
        ),
    )?;
    let domain = gui_domain();
    let service = format!("{domain}/{LABEL}");
    let _ = Command::new("launchctl")
        .args(["bootout", &service])
        .status();
    run_checked(Command::new("launchctl").args([
        "bootstrap",
        &domain,
        &path.display().to_string(),
    ]))?;
    run_checked(Command::new("launchctl").args(["enable", &service]))?;
    run_checked(Command::new("launchctl").args(["kickstart", "-k", &service]))?;
    Ok(true)
}

pub(in crate::lifecycle) fn unload_macos_daemon() {
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{}/{LABEL}", gui_domain())])
        .status();
}

pub(super) fn uninstall() -> ServiceResult<()> {
    unload_macos_daemon();
    Ok(sabine_runtime::remove_file_if_present(&agent_path()?)?)
}

/// Starts `program` as a launchd job of its own, so stopping the daemon's job
/// stops neither it nor the daemon it starts in turn.
pub(in crate::lifecycle) fn launch_outside_job(
    program: &Path,
    arguments: &[String],
) -> ServiceResult<()> {
    let path = sabine_runtime::sabine_data_dir().join("update-helper.plist");
    let program_arguments = std::iter::once(program.to_string_lossy().into_owned())
        .chain(arguments.iter().cloned())
        .collect::<Vec<_>>();
    fs::write(
        &path,
        property_list(
            HELPER_LABEL,
            &program_arguments,
            "<key>RunAtLoad</key><true/><key>AbandonProcessGroup</key><true/>",
        ),
    )?;
    let domain = gui_domain();
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{domain}/{HELPER_LABEL}")])
        .status();
    run_checked(Command::new("launchctl").args(["bootstrap", &domain, &path.display().to_string()]))
}

fn property_list(label: &str, program_arguments: &[String], options: &str) -> String {
    let arguments = program_arguments
        .iter()
        .map(|argument| format!("<string>{}</string>", xml_value(argument)))
        .collect::<String>();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>{label}</string><key>ProgramArguments</key><array>{arguments}</array>{options}</dict></plist>\n"
    )
}

fn agent_path() -> ServiceResult<std::path::PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| ServiceError::Update("HOME is not set".to_string()))?;
    Ok(Path::new(&home).join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

fn gui_domain() -> String {
    format!("gui/{}", unsafe { libc::getuid() })
}

fn xml_value(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
