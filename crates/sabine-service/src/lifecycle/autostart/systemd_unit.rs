use std::{
    fs, io,
    path::{Path, PathBuf},
};

use sabine_runtime::{path_in_root, system_dir};

use crate::{ServiceResult, install::stable_daemon_path};

pub(super) const UNIT: &str = "sabine.service";
const DESCRIPTION: &str = "Sabine runtime and app service";
const SYSTEM_UNIT_DIR: &str = "/usr/lib/systemd/user";
/// Also points a unit an older Sabine wrote for a user at the active daemon,
/// which then retires that unit.
const SYSTEM_EXEC_DROP_IN: &str = "sabine.service.d/10-system-daemon.conf";

pub(super) fn system_unit_installed() -> bool {
    Path::new(SYSTEM_UNIT_DIR).join(UNIT).is_file()
}

/// Installs the user unit every user's service manager starts at login, and
/// Sabine's desktop entry and icons, into the system rooted at `root`. Its
/// daemon is the active system version, which hands over to a newer
/// installation of the user's own when there is one.
pub fn install_system_unit(root: Option<&Path>) -> ServiceResult<()> {
    let units = path_in_root(root, Path::new(SYSTEM_UNIT_DIR));
    let daemon = system_dir().join("bin/current/sabine-service-daemon");
    let exec = format!("ExecStart={}\n", systemd_quote(&daemon.to_string_lossy()));
    fs::create_dir_all(&units)?;
    fs::write(units.join(UNIT), service_contents(&exec))?;
    let drop_in = units.join(SYSTEM_EXEC_DROP_IN);
    fs::create_dir_all(drop_in.parent().expect("the drop-in has a folder"))?;
    fs::write(drop_in, format!("[Service]\nExecStart=\n{exec}"))?;
    let wants = units.join("default.target.wants");
    fs::create_dir_all(&wants)?;
    let link = wants.join(UNIT);
    if link.symlink_metadata().is_ok() {
        fs::remove_file(&link)?;
    }
    std::os::unix::fs::symlink(Path::new("..").join(UNIT), link)?;
    crate::identity::install_for_system(root, &daemon)?;
    Ok(())
}

/// A unit for this user alone, used where the system ships none.
pub(super) fn user_unit_contents(daemon: &Path) -> String {
    let daemon = stable_daemon_path(daemon).unwrap_or_else(|| daemon.to_path_buf());
    let service = format!(
        "ExecStart={}\n{}",
        systemd_quote(&daemon.to_string_lossy()),
        systemd_environment()
    );
    format!(
        "{}\n[Install]\nWantedBy=default.target\n",
        service_contents(&service)
    )
}

fn service_contents(service: &str) -> String {
    format!("[Unit]\nDescription={DESCRIPTION}\n\n[Service]\n{service}Restart=on-failure\n")
}

/// Whether the unit at `path` is one Sabine wrote for this user rather than
/// one the person wrote or a mask.
pub(super) fn is_sabine_user_unit(path: &Path) -> bool {
    if !path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.is_file())
    {
        return false;
    }
    let Ok(contents) = fs::read_to_string(path) else {
        return false;
    };
    contents.starts_with(&format!(
        "[Unit]\nDescription={DESCRIPTION}\n\n[Service]\nExecStart="
    )) && contents.lines().any(|line| {
        line.strip_prefix("ExecStart=").is_some_and(|exec| {
            exec.ends_with("/sabine-service-daemon\"")
                && (exec.contains("/bin/versions/") || exec.contains("/bin/current/"))
        })
    })
}

pub(super) fn user_unit_path(config_home: &Path) -> PathBuf {
    config_home.join("systemd/user").join(UNIT)
}

pub(super) fn user_wants_path(config_home: &Path) -> PathBuf {
    config_home
        .join("systemd/user/default.target.wants")
        .join(UNIT)
}

pub(super) fn remove_file(path: &Path) -> io::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_units_sabine_wrote_are_retired() {
        let directory =
            std::env::temp_dir().join(format!("sabine-unit-test-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let older = directory.join("older.service");
        fs::write(
            &older,
            "[Unit]\nDescription=Sabine runtime and app service\n\n[Service]\nExecStart=\"/usr/lib/sabine/bin/versions/0.46/sabine-service-daemon\"\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n",
        )
        .unwrap();
        let current = directory.join("current.service");
        fs::write(
            &current,
            user_unit_contents(Path::new(
                "/home/someone/.local/share/sabine/bin/versions/0.48/sabine-service-daemon",
            )),
        )
        .unwrap();
        let own = directory.join("own.service");
        fs::write(
            &own,
            "[Unit]\nDescription=Sabine runtime and app service\n\n[Service]\nExecStart=/home/someone/sabine/target/release/sabine-service-daemon\n",
        )
        .unwrap();
        let mask = directory.join("mask.service");
        std::os::unix::fs::symlink("/dev/null", &mask).unwrap();

        assert!(is_sabine_user_unit(&older));
        assert!(is_sabine_user_unit(&current));
        assert!(!is_sabine_user_unit(&own));
        assert!(!is_sabine_user_unit(&mask));
        fs::remove_dir_all(directory).unwrap();
    }
}
