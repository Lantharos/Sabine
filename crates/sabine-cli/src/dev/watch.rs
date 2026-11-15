use std::{
    path::Path,
    sync::mpsc::{Receiver, TryRecvError, channel},
    time::{Duration, Instant},
};

use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{CreateKind, ModifyKind, RemoveKind},
};

const QUIET: Duration = Duration::from_millis(250);
const PACKAGE_FILES: [&str; 4] = ["Cargo.toml", "Cargo.lock", "build.rs", "Sabine.toml"];

/// Changes to the app's Rust package: its sources and its manifests.
pub(super) struct RustChanges {
    _watcher: RecommendedWatcher,
    events: Receiver<()>,
    pending: Option<Instant>,
}

impl RustChanges {
    pub(super) fn watch(package: &Path) -> Result<Self, String> {
        let (sender, events) = channel();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if event.is_ok_and(|event| is_rust_change(&event)) {
                let _ = sender.send(());
            }
        })
        .map_err(|error| format!("could not watch the Rust sources: {error}"))?;
        watcher
            .watch(package, RecursiveMode::NonRecursive)
            .map_err(|error| format!("could not watch {}: {error}", package.display()))?;
        let sources = package.join("src");
        if sources.is_dir() {
            watcher
                .watch(&sources, RecursiveMode::Recursive)
                .map_err(|error| format!("could not watch {}: {error}", sources.display()))?;
        }
        Ok(Self {
            _watcher: watcher,
            events,
            pending: None,
        })
    }

    /// Whether the sources changed and have been quiet for a moment since.
    pub(super) fn settled(&mut self) -> bool {
        loop {
            match self.events.try_recv() {
                Ok(()) => self.pending = Some(Instant::now()),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if self
            .pending
            .is_some_and(|changed| changed.elapsed() >= QUIET)
        {
            self.pending = None;
            return true;
        }
        false
    }
}

fn is_rust_change(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(CreateKind::File | CreateKind::Any)
            | EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Name(_) | ModifyKind::Any)
            | EventKind::Remove(RemoveKind::File | RemoveKind::Any)
    ) && event.paths.iter().any(|path| {
        !path
            .components()
            .any(|component| component.as_os_str() == "target")
            && (path.extension().is_some_and(|extension| extension == "rs")
                || path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| PACKAGE_FILES.contains(&name)))
    })
}
