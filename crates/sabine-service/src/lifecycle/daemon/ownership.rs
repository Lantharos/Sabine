use std::{fs, path::PathBuf, time::Duration};

use sabine_runtime::{FileLock, sabine_data_dir};
use serde::{Deserialize, Serialize};

use crate::ServiceResult;

pub(super) const DAEMON_STATE_FILE: &str = "daemon-state.json";

/// This process's claim to be the user's one daemon, held by an operating
/// system lock that is released when it exits.
pub(super) struct DaemonOwner {
    state_path: PathBuf,
    pub pid: u32,
    _lock: FileLock,
}

impl Drop for DaemonOwner {
    fn drop(&mut self) {
        if daemon_state().is_some_and(|state| state.pid == self.pid) {
            let _ = fs::remove_file(&self.state_path);
        }
    }
}

/// Claims the daemon role, or `None` while another daemon holds it.
pub(super) fn claim_daemon() -> ServiceResult<Option<DaemonOwner>> {
    let directory = sabine_data_dir();
    let lock = match FileLock::acquire(&directory.join("daemon.lock"), Duration::ZERO, |_| {}) {
        Ok(lock) => lock,
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let owner = DaemonOwner {
        state_path: directory.join(DAEMON_STATE_FILE),
        pid: std::process::id(),
        _lock: lock,
    };
    fs::write(
        &owner.state_path,
        serde_json::to_vec(&DaemonState {
            pid: owner.pid,
            version: crate::SABINE_VERSION.to_string(),
        })
        .expect("daemon state is serializable"),
    )?;
    Ok(Some(owner))
}

#[derive(Deserialize, Serialize)]
pub(super) struct DaemonState {
    pub pid: u32,
    pub version: String,
}

pub(super) fn daemon_state() -> Option<DaemonState> {
    serde_json::from_slice(&fs::read(sabine_data_dir().join(DAEMON_STATE_FILE)).ok()?).ok()
}
