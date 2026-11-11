use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use super::files::{claim, private_file, remove};
use super::state_path;

const SETTLE: Duration = Duration::from_millis(500);

enum Change {
    Keep(Option<Vec<u8>>),
    Forget,
}

/// Writes a window's state on a thread of its own, once it has stopped
/// changing for a moment, so the window never waits for the disk.
pub(super) struct Writer {
    changes: Sender<Change>,
    thread: JoinHandle<()>,
}

impl Writer {
    pub(super) fn start(folder: PathBuf, key: String, lock: Option<File>) -> Self {
        let (changes, received) = crossbeam_channel::unbounded();
        let thread = thread::spawn(move || {
            WriterThread { folder, key, lock }.run(&received);
        });
        Self { changes, thread }
    }

    pub(super) fn keep(&self, contents: Option<Vec<u8>>) {
        let _ = self.changes.send(Change::Keep(contents));
    }

    pub(super) fn forget(self) {
        let _ = self.changes.send(Change::Forget);
        let _ = self.thread.join();
    }
}

struct WriterThread {
    folder: PathBuf,
    key: String,
    lock: Option<File>,
}

impl WriterThread {
    fn run(&mut self, changes: &Receiver<Change>) {
        while let Ok(change) = changes.recv() {
            let Some(contents) = settled(change, changes) else {
                remove(&self.folder, &self.key);
                return;
            };
            if let Err(error) = self.write(contents) {
                sabine_runtime::report_error(
                    "window",
                    format!("could not keep what the window shows: {error}"),
                );
            }
        }
    }

    fn write(&mut self, contents: Option<Vec<u8>>) -> io::Result<()> {
        let path = state_path(&self.folder, &self.key);
        let Some(contents) = contents else {
            return sabine_runtime::remove_file_if_present(&path);
        };
        if self.lock.is_none() {
            self.lock = claim(&self.folder, &self.key)?;
        }
        if self.lock.is_none() {
            return Err(io::Error::other("another window holds its key"));
        }
        let temporary = path.with_extension("new");
        let mut file = private_file(&temporary)?;
        file.write_all(&contents)?;
        file.sync_all()?;
        drop(file);
        sabine_runtime::replace_file(&temporary, &path)
    }
}

/// The last of the changes that arrive until they pause, or `None` once the
/// window's state is to be forgotten.
fn settled(mut change: Change, changes: &Receiver<Change>) -> Option<Option<Vec<u8>>> {
    loop {
        let Change::Keep(contents) = change else {
            return None;
        };
        match changes.recv_timeout(SETTLE) {
            Ok(next) => change = next,
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                return Some(contents);
            }
        }
    }
}
