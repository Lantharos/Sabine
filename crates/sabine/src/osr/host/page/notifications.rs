use std::io::Write;
use std::sync::Arc;

use sabine_bridge::{NOTIFICATION_CLOSE_COMMAND, NOTIFICATION_SHOW_COMMAND};
use sabine_platform::{Notification, NotificationAction, NotificationEvent, Notifier, Urgency};
use serde::Deserialize;
use serde_json::Value;

use super::BridgeRequest;
use super::notification_images::picture_file;
use crate::bridge::frame::Frame;
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{HostControl, OsrHostEvent};
use crate::osr::protocol::{NOTIFICATION_EVENT_LINE, notification_event_to_json};

#[derive(Deserialize)]
struct Show {
    id: String,
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    silent: bool,
    #[serde(default)]
    urgency: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    actions: Vec<Action>,
}

#[derive(Deserialize)]
struct Action {
    id: String,
    label: String,
    #[serde(default)]
    reply: Option<String>,
}

#[derive(Deserialize)]
struct Close {
    id: String,
}

impl OsrNativeHost {
    pub(super) fn answer_notification(&mut self, request: &BridgeRequest) {
        let result = match request.command {
            NOTIFICATION_SHOW_COMMAND => serde_json::from_str::<Show>(request.payload)
                .map(|show| self.show_notification(self.page_notification(show))),
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

    fn page_notification(&self, show: Show) -> Notification {
        let app_id = self.config.app_id.as_deref().unwrap_or_default();
        let picture = |data_url: Option<String>| picture_file(app_id, &data_url?);
        Notification {
            id: show.id,
            title: show.title,
            body: show.body,
            silent: show.silent,
            urgency: show
                .urgency
                .as_deref()
                .and_then(Urgency::parse)
                .unwrap_or_default(),
            category: show.category.filter(|category| !category.is_empty()),
            icon: picture(show.icon),
            image: picture(show.image),
            actions: show
                .actions
                .into_iter()
                .take(Notification::MAX_ACTIONS)
                .map(|action| NotificationAction {
                    id: action.id,
                    label: action.label,
                    reply: action.reply,
                })
                .collect(),
        }
    }

    pub(in crate::osr::host) fn show_notification(&mut self, notification: Notification) {
        self.notifier().show(notification);
    }

    /// Connects to the desktop's notification service the first time the
    /// window shows a notification. Clicking one, or a button that doesn't
    /// take a reply, brings the window forward. The page hears what happens
    /// to its notifications, and the app all of it.
    fn notifier(&mut self) -> &Notifier {
        let relay = self.relay.clone();
        let sender = self.sender.clone();
        let proxy = self.proxy.clone();
        let app_id = self.config.app_id.clone().unwrap_or_default();
        let app_name = self.config.title.clone();
        self.notifier.get_or_insert_with(|| {
            Notifier::new(
                &app_id,
                &app_name,
                Arc::new(move |event| {
                    if let Some(activation_token) = focusing_token(&event)
                        && sender
                            .send(OsrHostEvent::HostControl(HostControl::Focus(
                                activation_token,
                            )))
                            .is_ok()
                    {
                        proxy.wake_up();
                    }
                    let payload = notification_event_to_json(&event);
                    relay.forward(Frame::encode(
                        &format!(
                            "SABINE_BRIDGE_EVENT\t\"{}\"\t{payload}",
                            page_event_name(&event)
                        ),
                        None,
                    ));
                    let mut output = std::io::stdout().lock();
                    let _ = writeln!(output, "{NOTIFICATION_EVENT_LINE}\t{payload}");
                    let _ = output.flush();
                }),
            )
        })
    }
}

fn focusing_token(event: &NotificationEvent) -> Option<Option<String>> {
    match event {
        NotificationEvent::Clicked {
            activation_token, ..
        }
        | NotificationEvent::Action {
            reply: None,
            activation_token,
            ..
        } => Some(activation_token.clone()),
        _ => None,
    }
}

fn page_event_name(event: &NotificationEvent) -> &'static str {
    match event {
        NotificationEvent::Clicked { .. } | NotificationEvent::Action { .. } => {
            "notification.click"
        }
        NotificationEvent::Closed(_) => "notification.close",
        NotificationEvent::Failed { .. } => "notification.error",
    }
}
