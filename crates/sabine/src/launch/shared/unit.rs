use std::{
    ffi::OsString,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use zbus::{Connection, Variant};

const SYSTEMD: &str = "org.freedesktop.systemd1";
const SYSTEMD_PATH: &str = "/org/freedesktop/systemd1";
const MANAGER: &str = "org.freedesktop.systemd1.Manager";
const UNIT_EXISTS: &str = "org.freedesktop.systemd1.UnitExists";

/// The Chromium a group of apps with the same flags shares: a socket-activated
/// user service the first of them starts, which serves them all until the
/// last one closes.
pub(super) struct RuntimeUnit {
    pub(super) key: String,
    pub(super) program: PathBuf,
    pub(super) arguments: Vec<OsString>,
    pub(super) environment: Vec<String>,
    pub(super) directory: PathBuf,
    pub(super) socket: PathBuf,
}

impl RuntimeUnit {
    fn name(&self, kind: &str) -> String {
        format!("sabine-runtime@{}.{kind}", self.key)
    }

    /// Starts the runtime's socket and service as transient units, unless an
    /// earlier launch already did.
    pub(super) async fn start(&self) -> Result<(), String> {
        let connection = Connection::session()
            .await
            .map_err(|error| error.to_string())?;
        let mut command = vec![self.program.as_os_str().as_bytes().to_vec()];
        command.extend(
            self.arguments
                .iter()
                .map(|argument| argument.as_bytes().to_vec()),
        );
        let command = command
            .into_iter()
            .map(String::from_utf8)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "the runtime's command line is not UTF-8".to_string())?;
        let program = command[0].clone();
        let service: Vec<(&str, Variant<'_>)> = vec![
            ("Description", Variant::from("Sabine runtime")),
            ("ExecStart", Variant::from(vec![(program, command, false)])),
            ("Environment", Variant::from(self.environment.clone())),
            (
                "WorkingDirectory",
                Variant::from(utf8(&self.directory)?.to_string()),
            ),
            (
                "ExecStopPost",
                Variant::from(vec![stop_socket(&self.name("socket"))]),
            ),
            ("Restart", Variant::from("on-failure")),
            ("CollectMode", Variant::from("inactive-or-failed")),
        ];
        let socket: Vec<(&str, Variant<'_>)> = vec![
            ("Description", Variant::from("Sabine runtime")),
            (
                "Listen",
                Variant::from(vec![(
                    "Stream".to_string(),
                    utf8(&self.socket)?.to_string(),
                )]),
            ),
            ("SocketMode", Variant::from(0o600u32)),
            ("RemoveOnStop", Variant::from(true)),
        ];
        let service_name = self.name("service");
        let auxiliary = vec![(service_name.as_str(), service)];
        let started = connection
            .call_method(
                Some(SYSTEMD),
                SYSTEMD_PATH,
                Some(MANAGER),
                "StartTransientUnit",
                &(self.name("socket"), "fail", socket, auxiliary),
            )
            .await;
        match started {
            Ok(_) => Ok(()),
            Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == UNIT_EXISTS => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// Takes the socket down with a runtime that quit because no app needed it,
/// so the sockets of runtimes an update replaced do not pile up. A runtime
/// that failed keeps its socket for systemd to start it again.
fn stop_socket(socket: &str) -> (String, Vec<String>, bool) {
    let shell = "/bin/sh".to_string();
    let script = format!(
        "[ \"$SERVICE_RESULT\" = success ] && exec systemctl --user --no-block stop {socket}"
    );
    (shell.clone(), vec![shell, "-c".to_string(), script], true)
}

fn utf8(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("{} is not UTF-8", path.display()))
}
