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
    pub urgency: Urgency,
    /// The kind of event, such as `email.arrived` or `im.received`, from the
    /// freedesktop notification categories. Only Linux notification servers
    /// read it.
    pub category: Option<String>,
}

impl Notification {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: String::new(),
            silent: false,
            urgency: Urgency::Normal,
            category: None,
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

    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }
}

/// How much a notification interrupts. Low ones go straight to the
/// notification list without a banner, and critical ones stay until they are
/// dismissed, as far as the desktop allows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Urgency {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::Critical => "critical",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "low" => Some(Self::Low),
            "normal" => Some(Self::Normal),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

/// What happened to a shown notification. A click carries the activation
/// token the desktop handed over with it, which lets the window come forward
/// on Wayland.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationEvent {
    Clicked {
        id: String,
        activation_token: Option<String>,
    },
    Closed(String),
    Failed {
        id: String,
        message: String,
    },
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
