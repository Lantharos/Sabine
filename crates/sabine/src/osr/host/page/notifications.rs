use std::sync::Arc;

use sabine_bridge::{NOTIFICATION_CLOSE_COMMAND, NOTIFICATION_SHOW_COMMAND};
use sabine_platform::{Notification, NotificationEvent, Notifier};
use serde::Deserialize;
use serde_json::{Value, json};

use super::BridgeRequest;
use crate::bridge::frame::Frame;
use crate::osr::host::native::OsrNativeHost;

#[derive(Deserialize)]
struct Show {
    id: String,
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    silent: bool,
}

#[derive(Deserialize)]
struct Close {
    id: String,
}

impl OsrNativeHost {
    pub(super) fn answer_notification(&mut self, request: &BridgeRequest) {
        let result = match request.command {
            NOTIFICATION_SHOW_COMMAND => {
                serde_json::from_str::<Show>(request.payload).map(|show| {
                    self.show_notification(Notification {
                        id: show.id,
                        title: show.title,
                        body: show.body,
                        silent: show.silent,
                    })
                })
            }
            NOTIFICATION_CLOSE_COMMAND => serde_json::from_str::<Close>(request.payload)
                .map(|close| self.notifier().close(&close.id)),
            command => {
                self.send_bridge_response(
                    request.browser_id,
                    request.request_id,
                    Err(format!("{command} is not a notification command")),
                );
                return;
            }
        };
        self.send_bridge_response(
            request.browser_id,
            request.request_id,
            result
                .map(|()| Value::Null)
                .map_err(|error| error.to_string()),
        );
    }

    pub(in crate::osr::host) fn show_notification(&mut self, notification: Notification) {
        self.notifier().show(notification);
    }

    /// Connects to the desktop's notification service the first time the
    /// window shows a notification.
    fn notifier(&mut self) -> &Notifier {
        let relay = self.relay.clone();
        let app_id = self.config.app_id.clone().unwrap_or_default();
        let app_name = self.config.title.clone();
        self.notifier.get_or_insert_with(|| {
            Notifier::new(
                &app_id,
                &app_name,
                Arc::new(move |event| relay.forward(Frame::encode(&event_line(event), None))),
            )
        })
    }
}

fn event_line(event: NotificationEvent) -> String {
    let (name, payload) = match event {
        NotificationEvent::Clicked(id) => ("notification.click", json!({ "id": id })),
        NotificationEvent::Closed(id) => ("notification.close", json!({ "id": id })),
        NotificationEvent::Failed { id, message } => (
            "notification.error",
            json!({ "id": id, "message": message }),
        ),
    };
    format!("SABINE_BRIDGE_EVENT\t\"{name}\"\t{payload}")
}
