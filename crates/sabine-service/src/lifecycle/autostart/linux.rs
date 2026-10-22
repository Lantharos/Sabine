use std::{
    fs, io,
    path::Path,
    process::{Command, Stdio},
};

use super::run_checked;
use super::systemd_unit::{
    UNIT, is_sabine_user_unit, remove_file, system_unit_installed, user_unit_contents,
    user_unit_path, user_wants_path,
};
use crate::{
    ServiceResult,
    app::desktop::{config_home, desktop_exec},
    identity::ICON_NAME,
};

const XDG_ENTRY: &str = "sabine-service.desktop";

/// Uses the user's systemd instance when there is one, so the daemon is
/// supervised and restarted; other sessions start it from an XDG autostart entry.
/// Where the system ships the unit, it runs the daemon and this user gets none.
pub(super) fn install(_executable: &Path, daemon: &Path) -> ServiceResult<bool> {
    let config = config_home()?;
    if systemd_main_pid().is_none() {
        remove_file(&user_unit_path(&config))?;
        write_xdg_entry(daemon)?;
        return Ok(false);
    }
    remove_file(&config.join("autostart").join(XDG_ENTRY))?;
    if !system_unit_installed() {
        install_user_unit(daemon)?;
        return Ok(true);
    }
    retire_shadowing_user_unit()?;
    if !belongs_to_an_installation(daemon) {
        return Ok(false);
    }
    run_checked(Command::new("systemctl").args(["--user", "unmask", UNIT]))?;
    start_unit(daemon)?;
    Ok(true)
}

pub(super) fn uninstall() -> ServiceResult<()> {
    let config = config_home()?;
    if system_unit_installed() {
        retire_shadowing_user_unit()?;
        let _ = run_checked(Command::new("systemctl").args(["--user", "mask", "--now", UNIT]));
    } else if user_unit_path(&config).is_file() {
        let _ = run_checked(Command::new("systemctl").args(["--user", "disable", "--now", UNIT]));
        fs::remove_file(user_unit_path(&config))?;
        let _ = run_checked(Command::new("systemctl").args(["--user", "daemon-reload"]));
    }
    remove_file(&config.join("autostart").join(XDG_ENTRY))?;
    Ok(())
}

/// Removes the unit an older Sabine wrote for this user, which would hide the
/// one the system ships.
pub(in crate::lifecycle) fn retire_shadowing_user_unit() -> ServiceResult<()> {
    let config = config_home()?;
    let unit = user_unit_path(&config);
    if !system_unit_installed() || !is_sabine_user_unit(&unit) {
        return Ok(());
    }
    fs::remove_file(unit)?;
    let wants = user_wants_path(&config);
    if wants
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.is_symlink())
    {
        fs::remove_file(wants)?;
    }
    run_checked(Command::new("systemctl").args(["--user", "daemon-reload"]))
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

/// The system unit starts the active system daemon, which hands over to the
/// user's own installation when that is newer; development builds run elsewhere.
fn belongs_to_an_installation(daemon: &Path) -> bool {
    let Ok(daemon) = fs::canonicalize(daemon) else {
        return false;
    };
    [
        sabine_runtime::sabine_data_dir(),
        sabine_runtime::system_dir(),
    ]
    .iter()
    .filter_map(|root| fs::canonicalize(root.join("bin/versions")).ok())
    .any(|versions| daemon.starts_with(versions))
}

fn install_user_unit(daemon: &Path) -> ServiceResult<()> {
    let path = user_unit_path(&config_home()?);
    fs::create_dir_all(path.parent().expect("the unit directory has a parent"))?;
    fs::write(&path, user_unit_contents(daemon))?;
    run_checked(Command::new("systemctl").args(["--user", "daemon-reload"]))?;
    run_checked(Command::new("systemctl").args(["--user", "enable", UNIT]))?;
    start_unit(daemon)
}

fn start_unit(daemon: &Path) -> ServiceResult<()> {
    let _ = run_checked(Command::new("systemctl").args(["--user", "reset-failed", UNIT]));
    run_checked(Command::new("systemctl").args(["--user", "start", UNIT]))?;
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
            "[Desktop Entry]\nType=Application\nName=Sabine\nComment=Sabine runtime and app service\nIcon={ICON_NAME}\nExec={}\nTerminal=false\nNoDisplay=true\n",
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
