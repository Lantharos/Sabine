use std::sync::{Arc, Mutex, MutexGuard};

use sabine_platform::{PlatformEvent, TrayActivation};
use zbus::{OwnedObjectPath, interface, object_server::SignalEmitter};

use super::MENU_PATH;
use crate::desktop::EventQueue;

pub(super) type Pixmap = (i32, i32, Vec<u8>);

/// What the tray shows for the app, which the menu and the controller share.
pub(super) struct Shown {
    pub(super) title: String,
    pub(super) tooltip: Option<String>,
    pub(super) pixmap: Pixmap,
}

/// The app's `org.kde.StatusNotifierItem`. A click activates the app; the
/// host opens the menu on a right click.
pub(super) struct Item {
    pub(super) id: String,
    pub(super) tray_id: String,
    pub(super) shown: Arc<Mutex<Shown>>,
    pub(super) events: EventQueue,
}

impl Item {
    fn shown(&self) -> MutexGuard<'_, Shown> {
        self.shown.lock().unwrap_or_else(|error| error.into_inner())
    }
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        &self.id
    }

    #[zbus(property)]
    fn title(&self) -> String {
        self.shown().title.clone()
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<Pixmap> {
        vec![self.shown().pixmap.clone()]
    }

    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<Pixmap>, String, String) {
        let shown = self.shown();
        let text = shown.tooltip.clone().unwrap_or_else(|| shown.title.clone());
        (String::new(), Vec::new(), text, String::new())
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).expect("the menu path is valid")
    }

    fn activate(&self, _x: i32, _y: i32) {
        let _ = self.events.send(PlatformEvent::Tray(TrayActivation::new(
            self.tray_id.clone(),
        )));
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {}

    fn context_menu(&self, _x: i32, _y: i32) {}

    fn scroll(&self, _delta: i32, _orientation: &str) {}

    #[zbus(signal)]
    pub(super) async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(super) async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
