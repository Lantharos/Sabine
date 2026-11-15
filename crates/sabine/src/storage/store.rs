use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex, MutexGuard, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

use crate::AppEnvironment;

const FILE: &str = "store.json";
/// How long the store waits for more changes before it writes them.
const QUIET: Duration = Duration::from_millis(250);
/// The longest changes stay unwritten while more keep coming.
const LATEST: Duration = Duration::from_secs(2);

/// A key of the store that changed: its new value, or `None` once deleted.
#[derive(Clone, Debug, PartialEq)]
pub struct StoreChange {
    pub key: String,
    pub value: Option<Value>,
}

type Listener = Arc<dyn Fn(&StoreChange) + Send + Sync>;

/// The app's own JSON values by key, kept in `store.json` in its data
/// folder, which its pages read and write too. Changes are written shortly
/// after they're made, all at once, and whenever the app ends.
#[derive(Clone)]
pub struct Store {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    entries: OnceLock<Mutex<Map<String, Value>>>,
    listeners: Mutex<Vec<Listener>>,
    dirty: AtomicBool,
    writing: Mutex<()>,
    flusher: OnceLock<crossbeam_channel::Sender<()>>,
}

static STORES: LazyLock<Mutex<HashMap<PathBuf, Weak<Inner>>>> = LazyLock::new(Mutex::default);

impl Store {
    /// The store of the app with `app_id`, as `Sabine.toml` names it. Every
    /// call for the same app returns the same store while one is in use.
    /// Development runs use the development app id's store.
    pub fn open(app_id: &str) -> Self {
        Self::at(AppEnvironment::current().app_data_dir(app_id).join(FILE))
    }

    fn at(path: PathBuf) -> Self {
        let mut stores = lock(&STORES);
        if let Some(inner) = stores.get(&path).and_then(Weak::upgrade) {
            return Self { inner };
        }
        let inner = Arc::new(Inner {
            path: path.clone(),
            entries: OnceLock::new(),
            listeners: Mutex::default(),
            dirty: AtomicBool::new(false),
            writing: Mutex::default(),
            flusher: OnceLock::new(),
        });
        stores.retain(|_, store| store.strong_count() > 0);
        stores.insert(path, Arc::downgrade(&inner));
        Self { inner }
    }

    /// The file the store keeps its values in.
    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub fn get(&self, key: &str) -> Option<Value> {
        self.inner.entries().get(key).cloned()
    }

    /// The value under `key` as a `T`, or `None` when the store has none.
    pub fn get_as<T: DeserializeOwned>(&self, key: &str) -> serde_json::Result<Option<T>> {
        self.get(key).map(serde_json::from_value).transpose()
    }

    /// Every key and its value.
    pub fn entries(&self) -> Map<String, Value> {
        self.inner.entries().clone()
    }

    /// Keeps `value` under `key`. Setting the value a key already holds
    /// changes nothing.
    pub fn set(&self, key: impl Into<String>, value: impl Serialize) -> serde_json::Result<()> {
        let key = key.into();
        let value = serde_json::to_value(value)?;
        let changed = {
            let mut entries = self.inner.entries();
            if entries.get(&key) == Some(&value) {
                false
            } else {
                entries.insert(key.clone(), value.clone());
                true
            }
        };
        if changed {
            self.inner.changed(&[StoreChange {
                key,
                value: Some(value),
            }]);
        }
        Ok(())
    }

    /// Forgets `key`. Returns whether the store held it.
    pub fn delete(&self, key: &str) -> bool {
        let removed = self.inner.entries().remove(key).is_some();
        if removed {
            self.inner.changed(&[StoreChange {
                key: key.to_string(),
                value: None,
            }]);
        }
        removed
    }

    /// Forgets every key.
    pub fn clear(&self) {
        let removed = std::mem::take(&mut *self.inner.entries());
        let changes = removed
            .into_iter()
            .map(|(key, _)| StoreChange { key, value: None })
            .collect::<Vec<_>>();
        if !changes.is_empty() {
            self.inner.changed(&changes);
        }
    }

    /// Calls `listener` after each change, whoever made it: the app or one
    /// of its pages.
    pub fn on_change(&self, listener: impl Fn(&StoreChange) + Send + Sync + 'static) {
        lock(&self.inner.listeners).push(Arc::new(listener));
    }

    /// Writes the changes not yet written now.
    pub fn flush(&self) -> std::io::Result<()> {
        self.inner.write()
    }
}

/// Writes every open store's changes, as the app ends.
pub(crate) fn flush_all() {
    let stores = lock(&STORES)
        .values()
        .filter_map(Weak::upgrade)
        .collect::<Vec<_>>();
    for store in stores {
        store.write_reporting();
    }
}

impl Inner {
    /// The values, read from the store's file the first time they're used.
    fn entries(&self) -> MutexGuard<'_, Map<String, Value>> {
        lock(self.entries.get_or_init(|| Mutex::new(self.read())))
    }

    fn read(&self) -> Map<String, Value> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Map::new(),
            Err(error) => {
                sabine_runtime::report_error(
                    "store",
                    format!("could not read {}: {error}", self.path.display()),
                );
                return Map::new();
            }
        };
        serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            let kept = self.path.with_extension("json.unreadable");
            let _ = std::fs::rename(&self.path, &kept);
            sabine_runtime::report_error(
                "store",
                format!(
                    "{} is not a JSON object, so the store starts empty and keeps it as {}: {error}",
                    self.path.display(),
                    kept.display()
                ),
            );
            Map::new()
        })
    }

    fn changed(self: &Arc<Self>, changes: &[StoreChange]) {
        self.dirty.store(true, Ordering::Release);
        let _ = self
            .flusher
            .get_or_init(|| start_flusher(Arc::downgrade(self)))
            .send(());
        let listeners = lock(&self.listeners).clone();
        for change in changes {
            for listener in &listeners {
                listener(change);
            }
        }
    }

    fn write(&self) -> std::io::Result<()> {
        let _writing = lock(&self.writing);
        if !self.dirty.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        let entries = self.entries().clone();
        sabine_runtime::write_json_atomic(&self.path, &entries).inspect_err(|_| {
            self.dirty.store(true, Ordering::Release);
        })
    }

    fn write_reporting(&self) {
        if let Err(error) = self.write() {
            sabine_runtime::report_error(
                "store",
                format!("could not write {}: {error}", self.path.display()),
            );
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.write_reporting();
    }
}

/// Writes the store once changes stop coming for [`QUIET`], or [`LATEST`]
/// after the first one at the latest. Ends with the store.
fn start_flusher(store: Weak<Inner>) -> crossbeam_channel::Sender<()> {
    let (changes, received) = crossbeam_channel::unbounded();
    std::thread::spawn(move || {
        while received.recv().is_ok() {
            let deadline = Instant::now() + LATEST;
            loop {
                let wait = QUIET.min(deadline.saturating_duration_since(Instant::now()));
                match received.recv_timeout(wait) {
                    Ok(()) if Instant::now() < deadline => {}
                    Ok(()) | Err(crossbeam_channel::RecvTimeoutError::Timeout) => break,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
                }
            }
            let Some(store) = store.upgrade() else {
                return;
            };
            store.write_reporting();
        }
    });
    changes
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{Store, StoreChange};

    #[test]
    fn keeps_values_across_stores_of_the_same_file() {
        let folder = std::env::temp_dir().join(format!("sabine-store-{}", std::process::id()));
        let path = folder.join("store.json");
        let changes = Arc::new(Mutex::new(Vec::new()));
        {
            let store = Store::at(path.clone());
            let seen = changes.clone();
            store.on_change(move |change| seen.lock().unwrap().push(change.clone()));
            store.set("theme", "dark").unwrap();
            store.set("theme", "dark").unwrap();
            store.set("size", 14).unwrap();
            assert!(store.delete("size"));
            assert!(!store.delete("size"));
        }
        assert_eq!(
            *changes.lock().unwrap(),
            [
                StoreChange {
                    key: "theme".into(),
                    value: Some("dark".into()),
                },
                StoreChange {
                    key: "size".into(),
                    value: Some(14.into()),
                },
                StoreChange {
                    key: "size".into(),
                    value: None,
                },
            ]
        );
        let reopened = Store::at(path);
        assert_eq!(
            reopened.get_as::<String>("theme").unwrap().as_deref(),
            Some("dark")
        );
        assert_eq!(reopened.entries().len(), 1);
        drop(reopened);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
