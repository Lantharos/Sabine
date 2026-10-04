use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::run_checked;
use crate::{ServiceResult, app::desktop::config_home};

const UNIT: &str = "sabine.service";
const XDG_ENTRY: &str = "sabine-service.desktop";

/// Uses the user's systemd instance when there is one, so the daemon is
/// supervised and restarted; other sessions start it from an XDG autostart entry.
pub(super) fn install(_executable: &Path, daemon: &Path) -> ServiceResult<bool> {
    if systemd_main_pid().is_some() {
        remove_file(&config_home()?.join("autostart").join(XDG_ENTRY))?;
        install_unit(daemon)?;
        Ok(true)
    } else {
        remove_file(&unit_path()?)?;
        write_xdg_entry(daemon)?;
        Ok(false)
    }
}

pub(super) fn uninstall() -> ServiceResult<()> {
    let unit = unit_path()?;
    if unit.is_file() {
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", UNIT])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        fs::remove_file(unit)?;
        let _ = run_checked(Command::new("systemctl").args(["--user", "daemon-reload"]));
    }
    remove_file(&config_home()?.join("autostart").join(XDG_ENTRY))?;
    Ok(())
}

/// Whether the daemon the service manager supervises is `expected`. Without a
/// user systemd instance nothing supervises the daemon, so any one will do.
pub(in crate::lifecycle) fn supervised_daemon_matches(expected: &Path) -> bool {
    match systemd_main_pid() {
        None => true,
        Some(pid) => {
            fs::canonicalize(format!("/proc/{pid}/exe")).ok() == fs::canonicalize(expected).ok()
        }
    }
}

fn install_unit(daemon: &Path) -> ServiceResult<()> {
    let path = unit_path()?;
    fs::create_dir_all(path.parent().expect("the unit directory has a parent"))?;
    fs::write(
        &path,
        format!(
            "[Unit]\nDescription=Sabine runtime and app service\n\n[Service]\nExecStart={}\n{}Restart=on-failure\n\n[Install]\nWantedBy=default.target\n",
            systemd_quote(&daemon.to_string_lossy()),
            systemd_environment()
        ),
    )?;
    run_checked(Command::new("systemctl").args(["--user", "daemon-reload"]))?;
    let _ = Command::new("systemctl")
        .args(["--user", "reset-failed", UNIT])
        .status();
    run_checked(Command::new("systemctl").args(["--user", "enable", "--now", UNIT]))?;
    if !supervised_daemon_matches(daemon) {
        run_checked(Command::new("systemctl").args(["--user", "restart", UNIT]))?;
    }
    Ok(())
}

fn write_xdg_entry(daemon: &Path) -> io::Result<()> {
    let directory = config_home()?.join("autostart");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join(XDG_ENTRY),
        format!(
            "[Desktop Entry]\nType=Application\nName=Sabine\nComment=Sabine runtime and app service\nExec={}\nTerminal=false\nNoDisplay=true\n",
            desktop_exec(&daemon.to_string_lossy())
        ),
    )
}

/// The daemon's process id under the user's systemd instance, or `None` when
/// there is no reachable user systemd.
fn systemd_main_pid() -> Option<u32> {
    let output = Command::new("systemctl")
        .args(["--user", "show", "--property=MainPID", "--value", UNIT])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn unit_path() -> io::Result<PathBuf> {
    Ok(config_home()?.join("systemd/user").join(UNIT))
}

fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn systemd_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('\"', "\\\"")
            .replace('%', "%%")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

fn desktop_exec(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    format!("\"{}\"", escaped.replace('\\', "\\\\"))
}

fn systemd_environment() -> String {
    ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME"]
        .into_iter()
        .filter_map(|name| {
            std::env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(|value| {
                    format!(
                        "Environment={}\n",
                        systemd_quote(&format!("{name}={}", value.to_string_lossy()))
                    )
                })
        })
        .collect()
}
