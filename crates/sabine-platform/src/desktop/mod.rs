mod appearance;
mod integration;
mod launcher;
mod notifications;
mod recent;
pub mod secrets;
pub mod shell;

pub use appearance::{Appearance, AppearanceWatcher};
pub use integration::{
    AutostartEntry, DeepLinkRegistration, GlobalShortcutActivation, GlobalShortcutFailure,
    GlobalShortcutRegistration, NativeMessagingHost, PlatformEvent, Shortcut, ShortcutModifiers,
    SingleInstanceActivation, SingleInstancePolicy, TrayActivation, TrayIcon, TrayMenuItem,
    TrayMenuItemKind,
};
pub use launcher::{AppBadge, AppProgress};
pub use notifications::{Notification, NotificationEvent, NotificationEvents, Notifier, Urgency};
pub use recent::RecentDocuments;
