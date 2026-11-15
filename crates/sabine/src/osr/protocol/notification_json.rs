//! Notifications on their way from the app to its window host, and what
//! happened to them on the way back to the app and its pages.

use sabine_platform::{Notification, NotificationAction, NotificationEvent, Urgency};
use serde_json::{Value, json};

/// A window host writes this, followed by a tab and the event's JSON, for
/// whatever happens to a notification the window showed.
pub(crate) const NOTIFICATION_EVENT_LINE: &str = "SABINE_NOTIFICATION_EVENT";

pub(crate) fn notification_to_json(notification: &Notification) -> Value {
    json!({
        "id": notification.id,
        "title": notification.title,
        "body": notification.body,
        "silent": notification.silent,
        "urgency": notification.urgency.as_str(),
        "category": notification.category,
        "icon": notification.icon,
        "image": notification.image,
        "actions": notification
            .actions
            .iter()
            .map(|action| json!({ "id": action.id, "label": action.label, "reply": action.reply }))
            .collect::<Vec<_>>(),
    })
}

pub(crate) fn notification_from_json(value: &str) -> Option<Notification> {
    let value = serde_json::from_str::<Value>(value).ok()?;
    let text = |key: &str| value.get(key).and_then(Value::as_str);
    Some(Notification {
        body: text("body").unwrap_or_default().to_string(),
        silent: value
            .get("silent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        urgency: text("urgency").and_then(Urgency::parse).unwrap_or_default(),
        category: text("category").map(str::to_string),
        icon: text("icon").map(Into::into),
        image: text("image").map(Into::into),
        actions: value
            .get("actions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|action| {
                Some(NotificationAction {
                    id: action.get("id")?.as_str()?.to_string(),
                    label: action.get("label")?.as_str()?.to_string(),
                    reply: action
                        .get("reply")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
            })
            .collect(),
        ..Notification::new(text("id")?, text("title")?)
    })
}

/// The event as pages and the app receive it. A click on the notification
/// itself has no `action`.
pub(crate) fn notification_event_to_json(event: &NotificationEvent) -> Value {
    match event {
        NotificationEvent::Clicked { id, .. } => {
            json!({ "type": "click", "id": id, "action": null, "reply": null })
        }
        NotificationEvent::Action {
            id, action, reply, ..
        } => json!({ "type": "click", "id": id, "action": action, "reply": reply }),
        NotificationEvent::Closed(id) => json!({ "type": "close", "id": id }),
        NotificationEvent::Failed { id, message } => {
            json!({ "type": "error", "id": id, "message": message })
        }
    }
}

/// The event a window host reported. The window host already brought its
/// window forward, so the app gets no activation token.
pub(crate) fn notification_event_from_json(value: &str) -> Option<NotificationEvent> {
    let value = serde_json::from_str::<Value>(value).ok()?;
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
    let id = text("id")?;
    Some(match value.get("type")?.as_str()? {
        "click" => match text("action") {
            Some(action) => NotificationEvent::Action {
                id,
                action,
                reply: text("reply"),
                activation_token: None,
            },
            None => NotificationEvent::Clicked {
                id,
                activation_token: None,
            },
        },
        "close" => NotificationEvent::Closed(id),
        "error" => NotificationEvent::Failed {
            id,
            message: text("message")?,
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_survive_the_trip_to_the_window_host() {
        let expected = Notification::new("mail-7", "Ayesha")
            .body("The photos are in the shared folder.")
            .silent(true)
            .urgency(Urgency::Low)
            .category("email.arrived")
            .icon("/tmp/ayesha.png")
            .action(NotificationAction::button("open", "Open"))
            .action(NotificationAction::reply("answer", "Reply", "Message"));
        let actual = notification_from_json(&notification_to_json(&expected).to_string());
        assert_eq!(actual, Some(expected));
    }

    #[test]
    fn replies_reach_the_app_with_their_text() {
        let reply = |activation_token: Option<&str>| NotificationEvent::Action {
            id: "mail-7".into(),
            action: "answer".into(),
            reply: Some("On my way".into()),
            activation_token: activation_token.map(Into::into),
        };
        let delivered = notification_event_from_json(
            &notification_event_to_json(&reply(Some("token"))).to_string(),
        );
        assert_eq!(delivered, Some(reply(None)));
    }
}
