use std::fs::{self, DirBuilder, File, OpenOptions, TryLockError};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use super::writer::Writer;
use super::{fresh_key, state_path};

const PRIVATE_FILE: u32 = 0o600;
const PRIVATE_FOLDER: u32 = 0o700;
const LARGEST_STATE: usize = 256 * 1024;
const EXTENSIONS: [&str; 3] = ["json", "new", "lock"];

fn prepare(folder: &Path) -> io::Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(PRIVATE_FOLDER)
        .create(folder)
}

pub(super) fn private_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(PRIVATE_FILE)
        .open(path)
}

/// Takes the lock a window holds on its key while it is open, unless another
/// window has it.
pub(super) fn claim(folder: &Path, key: &str) -> io::Result<Option<File>> {
    prepare(folder)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(PRIVATE_FILE)
        .open(folder.join(format!("{key}.lock")))?;
    match lock.try_lock() {
        Ok(()) => Ok(Some(lock)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

pub(super) fn remove(folder: &Path, key: &str) {
    for extension in EXTENSIONS {
        let _ = fs::remove_file(folder.join(format!("{key}.{extension}")));
    }
}

pub(super) fn forget_unclaimed(folder: &Path, requested: &[String]) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let left = entries.flatten().filter_map(|entry| {
        let name = entry.file_name().into_string().ok()?;
        name.strip_suffix(".json").map(str::to_owned)
    });
    for key in left.filter(|key| !requested.contains(key)) {
        if let Ok(Some(_lock)) = claim(folder, &key) {
            remove(folder, &key);
        }
    }
}

/// The state of one window: what it showed when it opened again, and the
/// writer that keeps what it shows now.
pub(crate) struct WindowMemory {
    folder: PathBuf,
    key: String,
    restored: Option<serde_json::Value>,
    lock: Option<File>,
    writer: Option<Writer>,
}

impl WindowMemory {
    /// Opens a new window, or the window with the key `restoring` that the
    /// desktop opens again, which starts out new when another window took it.
    pub(crate) fn open(folder: PathBuf, restoring: Option<String>) -> Self {
        let claimed = restoring.and_then(|key| {
            let lock = claim(&folder, &key).ok().flatten()?;
            Some((key, lock))
        });
        let restored = claimed.as_ref().and_then(|(key, _)| {
            let bytes = fs::read(state_path(&folder, key)).ok()?;
            serde_json::from_slice(&bytes).ok()
        });
        let (key, lock) =
            claimed.map_or_else(|| (fresh_key(), None), |(key, lock)| (key, Some(lock)));
        Self {
            folder,
            key,
            restored,
            lock,
            writer: None,
        }
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn restored(&self) -> Option<&serde_json::Value> {
        self.restored.as_ref()
    }

    /// Keeps `state` for the window, or nothing for `null`.
    pub(crate) fn keep(&mut self, state: &serde_json::Value) -> Result<(), String> {
        let contents = (!state.is_null())
            .then(|| serde_json::to_vec(state))
            .transpose()
            .map_err(|error| error.to_string())?;
        if contents
            .as_ref()
            .is_some_and(|contents| contents.len() > LARGEST_STATE)
        {
            return Err(format!(
                "a window keeps at most {} KiB of state",
                LARGEST_STATE / 1024
            ));
        }
        let writer = self.writer.get_or_insert_with(|| {
            Writer::start(self.folder.clone(), self.key.clone(), self.lock.take())
        });
        writer.keep(contents);
        Ok(())
    }

    /// Removes the window's state once people closed it.
    pub(crate) fn forget(&mut self) {
        match self.writer.take() {
            Some(writer) => writer.forget(),
            None if self.lock.take().is_some() => remove(&self.folder, &self.key),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{WindowMemory, claim, forget_unclaimed};
    use crate::window::restore::{fresh_key, state_path};

    fn folder() -> PathBuf {
        let folder = std::env::temp_dir().join(format!("sabine-windows-{}", fresh_key()));
        fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn left(folder: &Path, key: &str) {
        fs::write(state_path(folder, key), br#"{"page":"home"}"#).unwrap();
    }

    #[test]
    fn only_windows_nobody_holds_or_asked_for_lose_their_state() {
        let folder = folder();
        let [closed, open, asked] = [fresh_key(), fresh_key(), fresh_key()];
        for key in [&closed, &open, &asked] {
            left(&folder, key);
        }
        let _held = claim(&folder, &open).unwrap().unwrap();

        forget_unclaimed(&folder, std::slice::from_ref(&asked));

        assert!(!state_path(&folder, &closed).exists());
        assert!(state_path(&folder, &open).exists());
        assert!(state_path(&folder, &asked).exists());
        fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_window_opens_again_once_with_its_state() {
        let folder = folder();
        let key = fresh_key();
        left(&folder, &key);

        let mut restored = WindowMemory::open(folder.clone(), Some(key.clone()));
        let again = WindowMemory::open(folder.clone(), Some(key.clone()));

        assert_eq!(restored.key(), key);
        assert_eq!(
            restored.restored(),
            Some(&serde_json::json!({"page": "home"}))
        );
        assert_ne!(again.key(), key);
        assert_eq!(again.restored(), None);
        restored.forget();
        assert!(!state_path(&folder, &key).exists());
        fs::remove_dir_all(folder).unwrap();
    }
}
