use std::{collections::HashMap, sync::Mutex};

use ashpd::zbus::{Connection, Proxy, message::Message, zvariant::Value};
use futures_util::StreamExt;

use crate::notifications::{Notification, NotificationEvent, NotificationEvents, Urgency};

pub(super) const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const DEFAULT_ACTION: &str = "default";

/// `org.freedesktop.Notifications`, which numbers notifications itself;
/// `shown` maps those numbers to the page's ids.
pub(super) struct Freedesktop {
    proxy: Proxy<'static>,
    shown: Mutex<HashMap<u32, String>>,
}

/// The server's signals, in the order it sends them: a click sends the
/// notification's activation token just before the action.
enum Signal {
    ActivationToken(u32, String),
    Action(u32),
    Closed(u32),
}

impl Signal {
    fn read(message: &Message) -> Option<Self> {
        let body = message.body();
        match message.header().member()?.as_str() {
            "ActivationToken" => body
                .deserialize::<(u32, String)>()
                .ok()
                .map(|(number, token)| Self::ActivationToken(number, token)),
            "ActionInvoked" => body
                .deserialize::<(u32, String)>()
                .ok()
                .filter(|(_, action)| action == DEFAULT_ACTION)
                .map(|(number, _)| Self::Action(number)),
            "NotificationClosed" => body
                .deserialize::<(u32, u32)>()
                .ok()
                .map(|(number, _)| Self::Closed(number)),
            _ => None,
        }
    }
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
        let mut hints = HashMap::from([
            ("desktop-entry", Value::from(app_id)),
            ("suppress-sound", Value::from(notification.silent)),
            ("urgency", Value::from(urgency_level(notification.urgency))),
        ]);
        if let Some(category) = &notification.category {
            hints.insert("category", Value::from(category.as_str()));
        }
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
        let Ok(mut signals) = self.proxy.receive_all_signals().await else {
            return;
        };
        let mut tokens = HashMap::new();
        while let Some(message) = signals.next().await {
            match Signal::read(&message) {
                Some(Signal::ActivationToken(number, token)) => {
                    tokens.insert(number, token);
                }
                Some(Signal::Action(number)) => {
                    let activation_token = tokens.remove(&number);
                    if let Some(id) = self.shown().get(&number).cloned() {
                        events(NotificationEvent::Clicked {
                            id,
                            activation_token,
                        });
                    }
                }
                Some(Signal::Closed(number)) => {
                    tokens.remove(&number);
                    if let Some(id) = self.shown().remove(&number) {
                        events(NotificationEvent::Closed(id));
                    }
                }
                None => {}
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

fn urgency_level(urgency: Urgency) -> u8 {
    match urgency {
        Urgency::Low => 0,
        Urgency::Normal => 1,
        Urgency::Critical => 2,
    }
}
