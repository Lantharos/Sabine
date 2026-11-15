//! The badge and progress bar on the app's dock icon, through the
//! `com.canonical.Unity.LauncherEntry` signal that KDE Plasma, Dash to Dock,
//! Dash to Panel and other docks follow. Docks that start later ask the
//! entry for what it shows.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use sabine_platform::{AppBadge, AppProgress};
use tokio::sync::mpsc;
use zbus::{Variant, connection, interface, object_server::SignalEmitter};

enum Change {
    Badge(Option<AppBadge>),
    Progress(AppProgress),
}

#[derive(Clone)]
pub(in crate::desktop) struct LauncherEntry {
    changes: mpsc::UnboundedSender<Change>,
}

impl LauncherEntry {
    /// Connects to the session bus once the app first shows something.
    pub(in crate::desktop) fn new(app_id: &str) -> Self {
        let (changes, queue) = mpsc::unbounded_channel();
        let entry = Entry {
            app_uri: format!("application://{app_id}.desktop"),
            shown: Arc::default(),
        };
        let path = format!(
            "/com/canonical/unity/launcherentry/{}",
            app_id.replace(|character: char| !character.is_ascii_alphanumeric(), "_")
        );
        sabine_platform::async_runtime().spawn(async move {
            if let Err(error) = serve(entry, path, queue).await {
                sabine_runtime::report_error(
                    "desktop",
                    format!("could not show the dock badge: {error}"),
                );
            }
        });
        Self { changes }
    }

    pub(in crate::desktop) fn set_badge(&self, badge: Option<AppBadge>) {
        let _ = self.changes.send(Change::Badge(badge));
    }

    pub(in crate::desktop) fn set_progress(&self, progress: AppProgress) {
        let _ = self.changes.send(Change::Progress(progress));
    }
}

#[derive(Default)]
struct Shown {
    count: Option<u64>,
    progress: Option<f64>,
}

impl Shown {
    /// Whether the change shows something else.
    fn change(&mut self, change: Change) -> bool {
        let shown = (self.count, self.progress);
        match change {
            Change::Badge(badge) => {
                self.count = badge
                    .filter(|badge| badge.label().is_some())
                    .map(|AppBadge::Count(count)| count);
            }
            Change::Progress(progress) => self.progress = progress.value(),
        }
        (self.count, self.progress) != shown
    }

    fn properties(&self) -> HashMap<&'static str, Variant<'static>> {
        HashMap::from([
            ("count", Variant::from(self.count.unwrap_or(0) as i64)),
            ("count-visible", Variant::from(self.count.is_some())),
            ("progress", Variant::from(self.progress.unwrap_or(0.0))),
            ("progress-visible", Variant::from(self.progress.is_some())),
        ])
    }
}

struct Entry {
    app_uri: String,
    shown: Arc<Mutex<Shown>>,
}

#[interface(name = "com.canonical.Unity.LauncherEntry")]
impl Entry {
    fn query(&self) -> (String, HashMap<&'static str, Variant<'static>>) {
        (self.app_uri.clone(), lock(&self.shown).properties())
    }

    #[zbus(signal)]
    async fn update(
        emitter: &SignalEmitter<'_>,
        app_uri: &str,
        properties: HashMap<&str, Variant<'_>>,
    ) -> zbus::Result<()>;
}

async fn serve(
    entry: Entry,
    path: String,
    mut changes: mpsc::UnboundedReceiver<Change>,
) -> zbus::Result<()> {
    let Some(first) = changes.recv().await else {
        return Ok(());
    };
    let shown = Arc::clone(&entry.shown);
    let app_uri = entry.app_uri.clone();
    let connection = connection::Builder::session()
        .serve_at(path.as_str(), entry)
        .build()
        .await?;
    let emitter = SignalEmitter::new(&connection, path.as_str())?;
    let mut change = Some(first);
    while let Some(next) = change {
        let properties = {
            let mut shown = lock(&shown);
            shown.change(next).then(|| shown.properties())
        };
        if let Some(properties) = properties {
            Entry::update(&emitter, &app_uri, properties).await?;
        }
        change = changes.recv().await;
    }
    connection.close().await
}

fn lock(shown: &Mutex<Shown>) -> MutexGuard<'_, Shown> {
    shown.lock().unwrap_or_else(|error| error.into_inner())
}
