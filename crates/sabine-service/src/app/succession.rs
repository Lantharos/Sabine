//! Moves a running app to the copy of it just installed. An app running in
//! the background with no window open steps down when asked, and the copy
//! starts in its place the way logging in starts it. An app with a window
//! open keeps running until it is closed, and its next launch starts the copy.

use std::{
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use sabine_runtime::instance::{UPDATED, deliver, instance_socket};
use sabine_runtime::{FileLock, random_hex, report_error};

use crate::app::desktop::data_home;
use crate::{ServiceError, ServiceResult};

const AUTOSTART_ARGUMENTS: &str = "X-Sabine-Autostart-Arguments=";
const ANSWER_TIMEOUT: Duration = Duration::from_secs(4);
/// How long the app has to finish what it is doing and end.
const QUIT_TIMEOUT: Duration = Duration::from_secs(120);

/// Hands the running app `id` over to `executable`, its newly installed
/// program, when the app steps down for it.
pub(crate) fn hand_over(id: &str, executable: &Path) {
    if let Err(error) = try_hand_over(id, executable) {
        report_error(
            "update",
            format!("could not move the running {id} to its new version: {error}"),
        );
    }
}

fn try_hand_over(id: &str, executable: &Path) -> ServiceResult<()> {
    let socket = instance_socket(id)?;
    let Ok(stream) = UnixStream::connect(&socket) else {
        return Ok(());
    };
    let message = serde_json::json!({ UPDATED: true }).to_string();
    if !deliver(stream, message.as_bytes(), ANSWER_TIMEOUT)? {
        return Ok(());
    }
    drop(FileLock::acquire(
        &socket.with_extension("lock"),
        QUIT_TIMEOUT,
        |_| {},
    )?);
    start_in_background(id, executable, &background_arguments(id)?)
}

/// The arguments the app's launcher entry asks for when it starts at login.
fn background_arguments(id: &str) -> ServiceResult<Vec<String>> {
    let entry = data_home()?
        .join("applications")
        .join(format!("{id}.desktop"));
    let entry = std::fs::read_to_string(entry).unwrap_or_default();
    Ok(entry
        .lines()
        .find_map(|line| line.strip_prefix(AUTOSTART_ARGUMENTS))
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect())
}

/// Starts the app in a unit of its own, named the way desktops name the apps
/// they launch, so it outlives whatever installed it.
fn start_in_background(id: &str, executable: &Path, arguments: &[String]) -> ServiceResult<()> {
    if !user_service_manager().is_some_and(|socket| socket.exists()) {
        sabine_runtime::background_command(executable)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()?;
        return Ok(());
    }
    let status = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet"])
        .arg(format!(
            "--unit=app-sabine-{}@{}.service",
            id.replace('-', "\\x2d"),
            random_hex(4)?
        ))
        .arg("--")
        .arg(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(ServiceError::Update(format!(
            "systemd-run could not start {}: {status}",
            executable.display()
        )));
    }
    Ok(())
}

fn user_service_manager() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(|runtime| PathBuf::from(runtime).join("systemd/private"))
}
