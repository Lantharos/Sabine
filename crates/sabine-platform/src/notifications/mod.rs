#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod toast;
#[cfg(target_os = "macos")]
mod user_notifications;

use std::sync::Arc;

#[cfg(target_os = "linux")]
use linux::Backend;
#[cfg(target_os = "windows")]
use toast::Backend;
#[cfg(target_os = "macos")]
use user_notifications::Backend;

/// A desktop notification. Showing one with the id of a shown notification
/// replaces it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    pub id: String,
    pub title: String,
    pub body: String,
    pub silent: bool,
}

impl Notification {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: String::new(),
            silent: false,
        }
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    pub fn silent(mut self, silent: bool) -> Self {
        self.silent = silent;
        self
    }
}

/// What happened to a shown notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationEvent {
    Clicked(String),
    Closed(String),
    Failed { id: String, message: String },
}

pub type NotificationEvents = Arc<dyn Fn(NotificationEvent) + Send + Sync>;

/// Shows notifications through the desktop's own notification service:
/// `org.freedesktop.Notifications` on Linux, toasts on Windows and the user
/// notification center on macOS.
pub struct Notifier {
    backend: Backend,
}

impl Notifier {
    pub fn new(app_id: &str, app_name: &str, events: NotificationEvents) -> Self {
        Self {
            backend: Backend::new(app_id, app_name, events),
        }
    }

    pub fn show(&self, notification: Notification) {
        self.backend.show(notification);
    }

    pub fn close(&self, id: &str) {
        self.backend.close(id);
    }
}
