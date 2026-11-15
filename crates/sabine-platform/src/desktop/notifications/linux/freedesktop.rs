use std::{collections::HashMap, sync::Mutex};

use futures_util::StreamExt;
use zbus::{Connection, Proxy, Variant, message::Message};

use crate::{Notification, NotificationEvent, NotificationEvents, Urgency};

pub(super) const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const DEFAULT_ACTION: &str = "default";
/// The action of the text field servers with the `inline-reply` capability
/// show below a notification, of which there is one.
const INLINE_REPLY_ACTION: &str = "inline-reply";

/// `org.freedesktop.Notifications`, which numbers notifications itself;
/// `shown` maps those numbers to what the app shows.
pub(super) struct Freedesktop {
    proxy: Proxy<'static>,
    inline_reply: bool,
    shown: Mutex<HashMap<u32, Shown>>,
}

/// A notification on screen: the app's id for it, the ids of its buttons in
/// the order the server knows them by, and its inline reply's.
struct Shown {
    id: String,
    actions: Vec<String>,
    reply: Option<String>,
}

/// The server's signals, in the order it sends them: a click sends the
/// notification's activation token just before the action.
enum Signal {
    ActivationToken(u32, String),
    Action(u32, String),
    Replied(u32, String),
    Closed(u32),
}

impl Signal {
    fn read(message: &Message) -> Option<Self> {
        let body = message.body();
        let text = || body.deserialize::<(u32, String)>().ok();
        match message.header().member()?.as_str() {
            "ActivationToken" => text().map(|(number, token)| Self::ActivationToken(number, token)),
            "ActionInvoked" => text().map(|(number, action)| Self::Action(number, action)),
            "NotificationReplied" => text().map(|(number, reply)| Self::Replied(number, reply)),
            "NotificationClosed" => body
                .deserialize::<(u32, u32)>()
                .ok()
                .map(|(number, _)| Self::Closed(number)),
            _ => None,
        }
    }
}

impl Freedesktop {
    pub(super) async fn connect(connection: &Connection) -> zbus::Result<Self> {
        let proxy = Proxy::new(connection, SERVICE, PATH, SERVICE).await?;
        let capabilities: Vec<String> =
            proxy.call("GetCapabilities", &()).await.unwrap_or_default();
        Ok(Self {
            proxy,
            inline_reply: capabilities
                .iter()
                .any(|capability| capability == INLINE_REPLY_ACTION),
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
            ("desktop-entry", Variant::from(app_id)),
            ("suppress-sound", Variant::from(notification.silent)),
            (
                "urgency",
                Variant::from(urgency_level(notification.urgency)),
            ),
        ]);
        if let Some(category) = &notification.category {
            hints.insert("category", Variant::from(category.as_str()));
        }
        let image = notification.image.as_deref().map(path_text);
        if let Some(image) = &image {
            hints.insert("image-path", Variant::from(image.as_str()));
        }
        let mut shown = Shown {
            id: notification.id.clone(),
            actions: Vec::new(),
            reply: None,
        };
        let mut actions = vec![DEFAULT_ACTION.to_string(), String::new()];
        for action in notification.shown_actions() {
            match &action.reply {
                Some(placeholder) if self.inline_reply && shown.reply.is_none() => {
                    shown.reply = Some(action.id.clone());
                    actions.extend([INLINE_REPLY_ACTION.to_string(), action.label.clone()]);
                    hints.insert(
                        "x-kde-reply-placeholder-text",
                        Variant::from(placeholder.as_str()),
                    );
                    hints.insert(
                        "x-kde-reply-submit-button-text",
                        Variant::from(action.label.as_str()),
                    );
                }
                _ => {
                    actions.extend([shown.actions.len().to_string(), action.label.clone()]);
                    shown.actions.push(action.id.clone());
                }
            }
        }
        let icon = notification.icon.as_deref().map(path_text);
        let number: u32 = self
            .proxy
            .call(
                "Notify",
                &(
                    app_name,
                    replaces,
                    icon.as_deref().unwrap_or_default(),
                    notification.title.as_str(),
                    notification.body.as_str(),
                    actions,
                    hints,
                    -1_i32,
                ),
            )
            .await
            .map_err(|error| error.to_string())?;
        self.shown().insert(number, shown);
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
            let event = match Signal::read(&message) {
                Some(Signal::ActivationToken(number, token)) => {
                    tokens.insert(number, token);
                    None
                }
                Some(Signal::Action(number, action)) => {
                    let activation_token = tokens.remove(&number);
                    self.shown().get(&number).and_then(|shown| {
                        if action == DEFAULT_ACTION {
                            return Some(NotificationEvent::Clicked {
                                id: shown.id.clone(),
                                activation_token,
                            });
                        }
                        Some(NotificationEvent::Action {
                            id: shown.id.clone(),
                            action: shown.actions.get(action.parse::<usize>().ok()?)?.clone(),
                            reply: None,
                            activation_token,
                        })
                    })
                }
                Some(Signal::Replied(number, reply)) => {
                    self.shown().get(&number).and_then(|shown| {
                        Some(NotificationEvent::Action {
                            id: shown.id.clone(),
                            action: shown.reply.clone()?,
                            reply: Some(reply),
                            activation_token: tokens.remove(&number),
                        })
                    })
                }
                Some(Signal::Closed(number)) => {
                    tokens.remove(&number);
                    self.shown()
                        .remove(&number)
                        .map(|shown| NotificationEvent::Closed(shown.id))
                }
                None => None,
            };
            if let Some(event) = event {
                events(event);
            }
        }
    }

    fn shown(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Shown>> {
        self.shown.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn number_of(&self, id: &str) -> Option<u32> {
        self.shown()
            .iter()
            .find_map(|(number, shown)| (shown.id == id).then_some(*number))
    }
}

fn path_text(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

fn urgency_level(urgency: Urgency) -> u8 {
    match urgency {
        Urgency::Low => 0,
        Urgency::Normal => 1,
        Urgency::Critical => 2,
    }
}
