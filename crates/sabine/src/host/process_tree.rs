use std::{
    process::{Child, Command, ExitStatus},
    time::Duration,
};

pub(crate) struct ManagedChild {
    id: u32,
    exit: crossbeam_channel::Receiver<std::io::Result<ExitStatus>>,
    group: ProcessGroup,
}

impl ManagedChild {
    pub(crate) fn new(
        mut child: Child,
        exited: crossbeam_channel::Sender<u32>,
    ) -> std::io::Result<Self> {
        let id = child.id();
        let group = match ProcessGroup::register(id) {
            Ok(group) => group,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let (exit_sender, exit) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let mut child = child;
            let status = child.wait();
            let _ = exit_sender.send(status);
            let _ = exited.send(id);
        });
        Ok(Self { id, exit, group })
    }

    pub(crate) fn id(&self) -> u32 {
        self.id
    }

    pub(crate) fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        match self.exit.try_recv() {
            Ok(status) => self.finish(status).map(Some),
            Err(crossbeam_channel::TryRecvError::Empty) => Ok(None),
            Err(crossbeam_channel::TryRecvError::Disconnected) => Err(std::io::Error::other(
                "OSR host waiter exited without a status",
            )),
        }
    }

    pub(crate) fn terminate(&mut self) -> Option<ExitStatus> {
        self.group.terminate();
        match self.exit.recv_timeout(Duration::from_millis(250)) {
            Ok(status) => return self.finish(status).ok(),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return None,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
        self.group.kill();
        let status = self.exit.recv_timeout(Duration::from_secs(5)).ok()?.ok();
        self.group.unregister();
        status
    }

    fn finish(&self, status: std::io::Result<ExitStatus>) -> std::io::Result<ExitStatus> {
        self.group.unregister();
        status
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        self.group.unregister();
    }
}

/// Starts a child that ends with this process, such as a window host.
pub(crate) fn spawn_child(mut command: Command) -> std::io::Result<Child> {
    platform::prepare_child_command(&mut command, true);
    #[cfg(target_os = "linux")]
    return spawner::spawn(command);
    #[cfg(not(target_os = "linux"))]
    command.spawn()
}

/// Prepare a child that may outlive this process when siblings still need it
/// (shared CEF process-singleton). Still gets a fresh process group.
pub(crate) fn prepare_detachable_child_command(command: &mut Command) {
    platform::prepare_child_command(command, false);
}

#[cfg(target_os = "windows")]
#[path = "process_tree/windows.rs"]
mod platform;
#[cfg(unix)]
#[path = "process_tree/unix.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "process_tree/spawner.rs"]
mod spawner;

use platform::ProcessGroup;
