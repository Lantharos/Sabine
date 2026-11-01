mod appearance;
mod integration;
mod notifications;
pub mod shell;

pub use appearance::{Appearance, AppearanceWatcher};
pub use integration::{
    AutostartEntry, DeepLinkRegistration, GlobalShortcutActivation, GlobalShortcutFailure,
    GlobalShortcutRegistration, NativeMessagingHost, PlatformEvent, Shortcut, ShortcutModifiers,
    SingleInstanceActivation, SingleInstancePolicy, TrayActivation, TrayIcon, TrayMenuItem,
    TrayMenuItemKind,
};
pub use notifications::{Notification, NotificationEvent, NotificationEvents, Notifier, Urgency};
