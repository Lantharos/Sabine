use std::{collections::HashMap, sync::Mutex};

use ashpd::zbus::{Connection, Proxy, zvariant::Value};
use futures_util::{StreamExt, future, stream};

use crate::notifications::{Notification, NotificationEvent, NotificationEvents};

pub(super) const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const DEFAULT_ACTION: &str = "default";

/// `org.freedesktop.Notifications`, which numbers notifications itself;
/// `shown` maps those numbers to the page's ids.
pub(super) struct Freedesktop {
    proxy: Proxy<'static>,
    shown: Mutex<HashMap<u32, String>>,
}

enum Signal {
    Action(u32),
    Closed(u32),
}

impl Freedesktop {
    pub(super) async fn connect(connection: &Connection) -> ashpd::zbus::Result<Self> {
        Ok(Self {
            proxy: Proxy::new(connection, SERVICE, PATH, SERVICE).await?,
            shown: Mutex::default(),
        })
    }

    pub(super) async fn show(
        &self,
        app_id: &str,
        app_name: &str,
        notification: Notification,
    ) -> Result<(), String> {
        let replaces = self.number_of(&notification.id).unwrap_or(0);
        let hints = HashMap::from([
            ("desktop-entry", Value::from(app_id)),
            ("suppress-sound", Value::from(notification.silent)),
        ]);
        let number: u32 = self
            .proxy
            .call(
                "Notify",
                &(
                    app_name,
                    replaces,
                    "",
                    notification.title.as_str(),
                    notification.body.as_str(),
                    [DEFAULT_ACTION, ""].as_slice(),
                    hints,
                    -1_i32,
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        self.shown().insert(number, notification.id);
        Ok(())
    }

    pub(super) async fn close(&self, id: &str) {
        if let Some(number) = self.number_of(id) {
            let _ = self
                .proxy
                .call::<_, _, ()>("CloseNotification", &(number,))
                .await;
        }
    }

    pub(super) async fn listen(&self, events: &NotificationEvents) {
        let Ok(actions) = self.proxy.receive_signal("ActionInvoked").await else {
            return;
        };
        let Ok(closes) = self.proxy.receive_signal("NotificationClosed").await else {
            return;
        };
        let actions = actions.filter_map(|message| {
            future::ready(
                message
                    .body()
                    .deserialize::<(u32, String)>()
                    .ok()
                    .filter(|(_, action)| action == DEFAULT_ACTION)
                    .map(|(number, _)| Signal::Action(number)),
            )
        });
        let closes = closes.filter_map(|message| {
            future::ready(
                message
                    .body()
                    .deserialize::<(u32, u32)>()
                    .ok()
                    .map(|(number, _)| Signal::Closed(number)),
            )
        });
        let mut signals = stream::select(actions.boxed(), closes.boxed());
        while let Some(signal) = signals.next().await {
            match signal {
                Signal::Action(number) => {
                    if let Some(id) = self.shown().get(&number).cloned() {
                        events(NotificationEvent::Clicked(id));
                    }
                }
                Signal::Closed(number) => {
                    if let Some(id) = self.shown().remove(&number) {
                        events(NotificationEvent::Closed(id));
                    }
                }
            }
        }
    }

    fn shown(&self) -> std::sync::MutexGuard<'_, HashMap<u32, String>> {
        self.shown.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn number_of(&self, id: &str) -> Option<u32> {
        self.shown()
            .iter()
            .find_map(|(number, shown)| (shown == id).then_some(*number))
    }
}
