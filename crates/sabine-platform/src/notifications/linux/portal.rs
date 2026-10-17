use futures_util::StreamExt;
use zbus::{Connection, OwnedVariant, Proxy, Variant};

use crate::notifications::{Notification, NotificationEvent, NotificationEvents, Urgency};
use crate::portal::{self, Options, Results};

const NOTIFICATION: &str = "org.freedesktop.portal.Notification";
const DEFAULT_ACTION: &str = "app.sabine-notification";

/// The portal's Notification interface. It names the app through the
/// desktop entry of its id, so the app registers that id first; one without
/// an installed entry still shows its notifications.
pub(super) struct Portal {
    proxy: Proxy<'static>,
}

impl Portal {
    pub(super) async fn connect(connection: &Connection, app_id: &str) -> zbus::Result<Self> {
        let _ = portal::register_host_app(connection, app_id).await;
        Ok(Self {
            proxy: portal::proxy(connection, NOTIFICATION).await?,
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
        self.proxy
            .call::<_, _, ()>("AddNotification", &(notification.id.as_str(), shown))
            .await
            .map_err(|error| error.to_string())
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
            if name == DEFAULT_ACTION {
                events(NotificationEvent::Clicked {
                    id,
                    activation_token: activation_token(parameter),
                });
            }
        }
    }
}

fn priority(urgency: Urgency) -> &'static str {
    match urgency {
        Urgency::Low => "low",
        Urgency::Normal => "normal",
        Urgency::Critical => "urgent",
    }
}

/// The activation token in the platform data the portal passes with an
/// action, its last parameter.
fn activation_token(mut parameter: Vec<OwnedVariant>) -> Option<String> {
    let platform_data = Results::try_from(parameter.pop()?).ok()?;
    String::try_from(platform_data.get("activation-token")?.try_clone().ok()?).ok()
}
