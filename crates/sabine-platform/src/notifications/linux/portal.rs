use ashpd::desktop::notification::{Notification as PortalNotification, NotificationProxy};
use futures_util::StreamExt;

use crate::notifications::{Notification, NotificationEvent, NotificationEvents};

const DEFAULT_ACTION: &str = "app.sabine-notification";

/// The portal's Notification interface. It names the app through the
/// desktop entry of its id, so the app registers that id first; one without
/// an installed entry still shows its notifications.
pub(super) struct Portal {
    proxy: NotificationProxy,
}

impl Portal {
    pub(super) async fn connect(app_id: &str) -> ashpd::Result<Self> {
        if let Ok(app_id) = ashpd::AppID::try_from(app_id) {
            let _ = ashpd::register_host_app(app_id).await;
        }
        Ok(Self {
            proxy: NotificationProxy::new().await?,
        })
    }

    pub(super) async fn show(&self, notification: Notification) -> Result<(), String> {
        let shown = PortalNotification::new(&notification.title)
            .body(Some(notification.body.as_str()).filter(|body| !body.is_empty()))
            .default_action(DEFAULT_ACTION);
        self.proxy
            .add_notification(&notification.id, shown)
            .await
            .map_err(|error| error.to_string())
    }

    /// The portal reports no closed notifications, so closing one reports it
    /// right away.
    pub(super) async fn close(&self, id: &str, events: &NotificationEvents) {
        if self.proxy.remove_notification(id).await.is_ok() {
            events(NotificationEvent::Closed(id.to_string()));
        }
    }

    pub(super) async fn listen(&self, events: &NotificationEvents) {
        let Ok(mut actions) = self.proxy.receive_action_invoked().await else {
            return;
        };
        while let Some(action) = actions.next().await {
            if action.name() == DEFAULT_ACTION {
                events(NotificationEvent::Clicked(action.id().to_string()));
            }
        }
    }
}
