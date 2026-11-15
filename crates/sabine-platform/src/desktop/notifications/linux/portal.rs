use std::{collections::HashMap, sync::Mutex};

use futures_util::StreamExt;
use zbus::{Connection, OwnedVariant, Proxy, Variant};

use crate::portal::{self, Options, Results};
use crate::{Notification, NotificationEvent, NotificationEvents, Urgency};

const NOTIFICATION: &str = "org.freedesktop.portal.Notification";
const DEFAULT_ACTION: &str = "app.sabine-notification";
/// Each button's action, followed by its place among the notification's
/// buttons.
const BUTTON_ACTION: &str = "app.sabine-notification-button-";
const REPLY_PURPOSE: &str = "im.reply-with-text";

/// The portal's Notification interface. It names the app through the
/// desktop entry of its id, so the app registers that id first; one without
/// an installed entry still shows its notifications.
pub(super) struct Portal {
    proxy: Proxy<'static>,
    replies: bool,
    /// The ids of each shown notification's buttons, in order.
    buttons: Mutex<HashMap<String, Vec<String>>>,
}

impl Portal {
    pub(super) async fn connect(connection: &Connection, app_id: &str) -> zbus::Result<Self> {
        let _ = portal::register_host_app(connection, app_id).await;
        let proxy = portal::proxy(connection, NOTIFICATION).await?;
        let replies = proxy
            .get_property::<Results>("SupportedOptions")
            .await
            .ok()
            .and_then(|options| {
                Vec::<String>::try_from(options.get("button-purpose")?.try_clone().ok()?).ok()
            })
            .is_some_and(|purposes| purposes.iter().any(|purpose| purpose == REPLY_PURPOSE));
        Ok(Self {
            proxy,
            replies,
            buttons: Mutex::default(),
        })
    }

    pub(super) async fn show(&self, notification: Notification) -> Result<(), String> {
        let mut shown = Options::from([
            ("title", Variant::from(notification.title.as_str())),
            ("default-action", Variant::from(DEFAULT_ACTION)),
            ("priority", Variant::from(priority(notification.urgency))),
        ]);
        if !notification.body.is_empty() {
            shown.insert("body", Variant::from(notification.body.as_str()));
        }
        if let Some(icon) = notification
            .icon
            .as_deref()
            .and_then(|path| std::fs::read(path).ok())
        {
            shown.insert("icon", Variant::from(("bytes", Variant::from(icon))));
        }
        let buttons = notification
            .shown_actions()
            .enumerate()
            .map(|(place, action)| {
                let mut button = HashMap::from([
                    ("label", Variant::from(action.label.as_str())),
                    ("action", Variant::from(format!("{BUTTON_ACTION}{place}"))),
                ]);
                if action.reply.is_some() && self.replies {
                    button.insert("purpose", Variant::from(REPLY_PURPOSE));
                }
                button
            })
            .collect::<Vec<_>>();
        if !buttons.is_empty() {
            shown.insert("buttons", Variant::from(buttons));
        }
        let ids = notification
            .shown_actions()
            .map(|action| action.id.clone())
            .collect();
        self.proxy
            .call::<_, _, ()>("AddNotification", &(notification.id.as_str(), shown))
            .await
            .map_err(|error| error.to_string())?;
        self.buttons().insert(notification.id, ids);
        Ok(())
    }

    /// The portal reports no closed notifications, so closing one reports it
    /// right away.
    pub(super) async fn close(&self, id: &str, events: &NotificationEvents) {
        if self
            .proxy
            .call::<_, _, ()>("RemoveNotification", &(id,))
            .await
            .is_ok()
        {
            self.buttons().remove(id);
            events(NotificationEvent::Closed(id.to_string()));
        }
    }

    pub(super) async fn listen(&self, events: &NotificationEvents) {
        let Ok(mut actions) = self.proxy.receive_signal("ActionInvoked").await else {
            return;
        };
        while let Some(action) = actions.next().await {
            let Ok((id, name, parameter)) = action
                .body()
                .deserialize::<(String, String, Vec<OwnedVariant>)>()
            else {
                continue;
            };
            let Parameter {
                activation_token,
                reply,
            } = Parameter::read(parameter);
            let event = if name == DEFAULT_ACTION {
                Some(NotificationEvent::Clicked {
                    id,
                    activation_token,
                })
            } else {
                name.strip_prefix(BUTTON_ACTION)
                    .and_then(|place| place.parse::<usize>().ok())
                    .and_then(|place| self.buttons().get(&id)?.get(place).cloned())
                    .map(|action| NotificationEvent::Action {
                        id,
                        action,
                        reply,
                        activation_token,
                    })
            };
            if let Some(event) = event {
                events(event);
            }
        }
    }

    fn buttons(&self) -> std::sync::MutexGuard<'_, HashMap<String, Vec<String>>> {
        self.buttons
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

fn priority(urgency: Urgency) -> &'static str {
    match urgency {
        Urgency::Low => "low",
        Urgency::Normal => "normal",
        Urgency::Critical => "urgent",
    }
}

/// What the portal passes with an action: the platform data holding the
/// activation token, and the text of a reply. Where each one sits depends on
/// the kind of action, so they are told apart by their type.
struct Parameter {
    activation_token: Option<String>,
    reply: Option<String>,
}

impl Parameter {
    fn read(parameter: Vec<OwnedVariant>) -> Self {
        let mut read = Self {
            activation_token: None,
            reply: None,
        };
        for value in parameter {
            match value
                .try_clone()
                .ok()
                .and_then(|value| Results::try_from(value).ok())
            {
                Some(platform_data) => {
                    read.activation_token = platform_data
                        .get("activation-token")
                        .and_then(|token| String::try_from(token.try_clone().ok()?).ok());
                }
                None => read.reply = String::try_from(value).ok().or(read.reply),
            }
        }
        read
    }
}
