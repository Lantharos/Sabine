//! The tray icon on Linux: a StatusNotifierItem with a dbusmenu, which the
//! desktop's StatusNotifierWatcher shows.

mod item;
mod menu;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use futures_util::StreamExt;
use sabine_platform::{TrayIcon, TrayMenuItem};
use tokio::sync::mpsc;
use zbus::{Connection, connection, fdo::DBusProxy, object_server::SignalEmitter};

use super::icon;
use crate::desktop::{EventQueue, sanitize_id};
use item::{Item, Pixmap, Shown};
use menu::Menu;

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

enum Change {
    Icon(Option<PathBuf>),
    Tooltip(Option<String>),
    Menu(Vec<TrayMenuItem>),
    Remove,
}

/// Changes the app's tray icon while it runs. Changes apply in the order
/// they are made.
#[derive(Clone)]
pub struct TrayHandle {
    changes: mpsc::UnboundedSender<Change>,
}

impl TrayHandle {
    pub(in crate::desktop) fn spawn(icon: &TrayIcon, events: &EventQueue) -> Self {
        let (changes, queue) = mpsc::unbounded_channel();
        let icon = icon.clone();
        let events = events.clone();
        sabine_platform::async_runtime().spawn(async move {
            if let Err(error) = serve(icon, events, queue).await {
                sabine_runtime::report_error(
                    "desktop",
                    format!("could not show the tray icon: {error}"),
                );
            }
        });
        Self { changes }
    }

    /// Shows the image at `path`, or a plain dot without one.
    pub fn set_icon(&self, path: Option<PathBuf>, _template: bool) {
        let _ = self.changes.send(Change::Icon(path));
    }

    pub fn set_tooltip(&self, tooltip: Option<String>) {
        let _ = self.changes.send(Change::Tooltip(tooltip));
    }

    pub fn set_menu(&self, items: Vec<TrayMenuItem>) {
        let _ = self.changes.send(Change::Menu(items));
    }

    pub(in crate::desktop) fn remove(&self) {
        let _ = self.changes.send(Change::Remove);
    }
}

async fn serve(
    icon: TrayIcon,
    events: EventQueue,
    mut changes: mpsc::UnboundedReceiver<Change>,
) -> zbus::Result<()> {
    let shown = Arc::new(Mutex::new(Shown {
        title: icon.title.clone(),
        tooltip: icon.tooltip.clone(),
        pixmap: pixmap(icon.icon_path.clone()).await,
    }));
    let item = Item {
        id: sanitize_id(&icon.id),
        tray_id: icon.id.clone(),
        shown: Arc::clone(&shown),
        events: events.clone(),
    };
    let connection = connection::Builder::session()
        .serve_at(ITEM_PATH, item)
        .serve_at(MENU_PATH, Menu::new(icon.id, events, &icon.menu))
        .build()
        .await?;
    let mut watchers = DBusProxy::new(&connection)
        .await?
        .receive_name_owner_changed_with_args(&[(0, WATCHER)])
        .await?;
    register(&connection).await;
    let emitter = SignalEmitter::new(&connection, ITEM_PATH)?;
    loop {
        tokio::select! {
            change = changes.recv() => match change {
                Some(Change::Icon(path)) => {
                    lock(&shown).pixmap = pixmap(path).await;
                    Item::new_icon(&emitter).await?;
                }
                Some(Change::Tooltip(tooltip)) => {
                    lock(&shown).tooltip = tooltip;
                    Item::new_tool_tip(&emitter).await?;
                }
                Some(Change::Menu(items)) => {
                    let menu = connection.object_server().interface::<_, Menu>(MENU_PATH).await?;
                    let revision = menu.get_mut().await.show(&items);
                    Menu::layout_updated(menu.signal_emitter(), revision, 0).await?;
                }
                Some(Change::Remove) | None => break,
            },
            Some(watcher) = watchers.next() => {
                if watcher.args().is_ok_and(|args| args.new_owner().is_some()) {
                    register(&connection).await;
                }
            }
        }
    }
    connection.close().await
}

/// Asks the desktop's watcher to show the item. Without a watcher, the item
/// waits for one to start.
async fn register(connection: &Connection) {
    let Some(name) = connection.unique_name() else {
        return;
    };
    let _ = connection
        .call_method(
            Some(WATCHER),
            WATCHER_PATH,
            Some(WATCHER),
            "RegisterStatusNotifierItem",
            &(name.as_str(),),
        )
        .await;
}

async fn pixmap(path: Option<PathBuf>) -> Pixmap {
    tokio::task::spawn_blocking(move || icon::load(path.as_deref()).into_pixmap())
        .await
        .unwrap_or_else(|_| icon::load(None).into_pixmap())
}

fn lock(shown: &Mutex<Shown>) -> MutexGuard<'_, Shown> {
    shown.lock().unwrap_or_else(|error| error.into_inner())
}
