use std::{fs, path::Path, process::Command};

use super::run_checked;
use crate::{ServiceError, ServiceResult};

const LABEL: &str = "net.lantharos.sabine";

pub(super) fn install(_executable: &Path, daemon: &Path) -> ServiceResult<bool> {
    let path = agent_path()?;
    fs::create_dir_all(path.parent().expect("LaunchAgents has a parent"))?;
    fs::write(
        &path,
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>{LABEL}</string><key>ProgramArguments</key><array><string>{}</string></array><key>RunAtLoad</key><true/><key>KeepAlive</key><true/></dict></plist>\n",
            xml_value(&daemon.to_string_lossy())
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
    match fs::remove_file(agent_path()?) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
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
