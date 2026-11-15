#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod toast;
#[cfg(target_os = "macos")]
mod user_notifications;

use std::path::PathBuf;
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
    /// A picture in place of the app's icon, such as the sender's photo.
    /// macOS always shows the app's icon.
    pub icon: Option<PathBuf>,
    /// A larger picture shown in the notification. The XDG desktop portal
    /// shows none.
    pub image: Option<PathBuf>,
    /// The buttons below the notification. Desktops show the first
    /// [`Self::MAX_ACTIONS`].
    pub actions: Vec<NotificationAction>,
}

impl Notification {
    /// The most buttons a notification shows on every desktop.
    pub const MAX_ACTIONS: usize = 5;

    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: String::new(),
            silent: false,
            urgency: Urgency::Normal,
            category: None,
            icon: None,
            image: None,
            actions: Vec::new(),
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

    pub fn icon(mut self, path: impl Into<PathBuf>) -> Self {
        self.icon = Some(path.into());
        self
    }

    pub fn image(mut self, path: impl Into<PathBuf>) -> Self {
        self.image = Some(path.into());
        self
    }

    pub fn action(mut self, action: NotificationAction) -> Self {
        self.actions.push(action);
        self
    }

    fn shown_actions(&self) -> impl Iterator<Item = &NotificationAction> {
        self.actions.iter().take(Self::MAX_ACTIONS)
    }
}

/// A button on a notification. A reply button lets people type an answer
/// into the notification where the desktop offers that, and is a plain
/// button elsewhere.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
    /// The placeholder of a reply button's text field, which makes it a
    /// reply button; empty for none.
    pub reply: Option<String>,
}

impl NotificationAction {
    pub fn button(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            reply: None,
        }
    }

    pub fn reply(
        id: impl Into<String>,
        label: impl Into<String>,
        placeholder: impl Into<String>,
    ) -> Self {
        Self {
            reply: Some(placeholder.into()),
            ..Self::button(id, label)
        }
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

/// What happened to a shown notification. Clicks carry the activation token
/// the desktop handed over with them, which lets the window come forward on
/// Wayland.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationEvent {
    Clicked {
        id: String,
        activation_token: Option<String>,
    },
    /// A button was clicked, or a reply sent with the text people typed.
    Action {
        id: String,
        action: String,
        reply: Option<String>,
        activation_token: Option<String>,
    },
    Closed(String),
    Failed {
        id: String,
        message: String,
    },
}

impl NotificationEvent {
    pub fn id(&self) -> &str {
        match self {
            Self::Clicked { id, .. }
            | Self::Action { id, .. }
            | Self::Closed(id)
            | Self::Failed { id, .. } => id,
        }
    }
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
