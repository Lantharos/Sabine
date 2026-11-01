use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::process::{ProcessIdentity, process_alive};
use crate::{RuntimeError, user_runtime_path};

pub struct RuntimeLease {
    path: Option<PathBuf>,
}

impl RuntimeLease {
    pub fn acquire(runtime_dir: &Path) -> Result<Self, RuntimeError> {
        if !runtime_dir.starts_with(user_runtime_path()) {
            return Ok(Self { path: None });
        }
        let _lock = runtime_mutation_lock()?;
        if !crate::discovery::host::runtime_is_valid(runtime_dir) {
            return Err(RuntimeError::NotFound(format!(
                "runtime was removed or became unavailable: {}",
                runtime_dir.display()
            )));
        }
        let directory = runtime_dir.join(".leases");
        std::fs::create_dir_all(&directory)?;
        let owner = ProcessIdentity::current().ok_or_else(|| {
            RuntimeError::InstallationFailed("could not read when this process started".into())
        })?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = directory.join(format!("{}-{nonce}.lease", owner.pid));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        writeln!(file, "pid={}\nstarted={}", owner.pid, owner.started)?;
        file.sync_all()?;
        Ok(Self { path: Some(path) })
    }
}

impl Drop for RuntimeLease {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Whether the process that wrote `lease` still runs. Apps built with Sabine
/// 0.65 and earlier record only their process id.
fn lease_holder_running(lease: &str) -> bool {
    let value = |key: &str| {
        lease.lines().find_map(|line| {
            line.strip_prefix(key)
                .and_then(|value| value.trim().parse::<u64>().ok())
        })
    };
    let Some(pid) = value("pid=").and_then(|pid| u32::try_from(pid).ok()) else {
        return false;
    };
    match value("started=") {
        Some(started) => ProcessIdentity { pid, started }.is_running(),
        None => process_alive(pid),
    }
}

pub(crate) fn runtime_is_leased(runtime_dir: &Path) -> Result<bool, RuntimeError> {
    let directory = runtime_dir.join(".leases");
    if !directory.is_dir() {
        return Ok(false);
    }
    let mut leased = false;
    for entry in std::fs::read_dir(&directory)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        if std::fs::read_to_string(&path).is_ok_and(|lease| lease_holder_running(&lease)) {
            leased = true;
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    if !leased {
        let _ = std::fs::remove_dir(directory);
    }
    Ok(leased)
}

pub(crate) fn runtime_mutation_lock() -> std::io::Result<crate::FileLock> {
    crate::FileLock::acquire(
        &user_runtime_path().join(".mutation.lock"),
        std::time::Duration::from_secs(10),
        |_| {},
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reused_process_id_does_not_hold_a_lease() {
        let owner = ProcessIdentity::current().unwrap();
        let lease = |started: u64| format!("pid={}\nstarted={started}", owner.pid);
        assert!(lease_holder_running(&lease(owner.started)));
        assert!(!lease_holder_running(&lease(owner.started + 1)));
    }
}
