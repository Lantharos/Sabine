use std::{fs, path::PathBuf, time::Duration};

use sabine_runtime::{FileLock, ProcessIdentity, process_alive, sabine_data_dir};
use serde::{Deserialize, Serialize};

use crate::{ServiceError, ServiceResult};

pub(super) const DAEMON_STATE_FILE: &str = "daemon-state.json";

/// This process's claim to be the user's one daemon, held by an operating
/// system lock that is released when it exits.
pub(super) struct DaemonOwner {
    state_path: PathBuf,
    pub identity: ProcessIdentity,
    _lock: FileLock,
}

impl Drop for DaemonOwner {
    fn drop(&mut self) {
        if daemon_state().is_some_and(|state| state.is(self.identity)) {
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
    let identity = ProcessIdentity::current().ok_or_else(|| {
        ServiceError::Update("could not read when the Sabine service started".to_string())
    })?;
    let owner = DaemonOwner {
        state_path: directory.join(DAEMON_STATE_FILE),
        identity,
        _lock: lock,
    };
    sabine_runtime::write_json_atomic(
        &owner.state_path,
        &DaemonState {
            pid: identity.pid,
            started: Some(identity.started),
            version: crate::SABINE_VERSION.to_string(),
        },
    )?;
    Ok(Some(owner))
}

#[derive(Deserialize, Serialize)]
pub(super) struct DaemonState {
    pub pid: u32,
    /// When the daemon started. Daemons of Sabine 0.90 and earlier record
    /// only their process id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    started: Option<u64>,
    pub version: String,
}

impl DaemonState {
    /// Whether the daemon that wrote this state still runs, and not merely a
    /// later process the system gave its id.
    pub fn is_running(&self) -> bool {
        match self.started {
            Some(started) => ProcessIdentity {
                pid: self.pid,
                started,
            }
            .is_running(),
            None => process_alive(self.pid),
        }
    }

    pub fn is(&self, identity: ProcessIdentity) -> bool {
        self.pid == identity.pid && self.started == Some(identity.started)
    }
}

pub(super) fn daemon_state() -> Option<DaemonState> {
    serde_json::from_slice(&fs::read(sabine_data_dir().join(DAEMON_STATE_FILE)).ok()?).ok()
}
