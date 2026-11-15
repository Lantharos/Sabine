mod appearance;
mod idle;
mod integration;
mod launcher;
mod notifications;
mod power;
mod recent;
pub mod secrets;
pub mod shell;
mod sleep;

pub use appearance::{Appearance, AppearanceWatcher};
pub use idle::{IdleEvents, IdleState, IdleWatch};
pub use integration::{
    AutostartEntry, DeepLinkRegistration, GlobalShortcutActivation, GlobalShortcutFailure,
    GlobalShortcutRegistration, NativeMessagingHost, PlatformEvent, Shortcut, ShortcutModifiers,
    SingleInstanceActivation, SingleInstancePolicy, TrayActivation, TrayIcon, TrayMenuItem,
    TrayMenuItemKind,
};
pub use launcher::{AppBadge, AppProgress};
pub use notifications::{
    Notification, NotificationAction, NotificationEvent, NotificationEvents, Notifier, Urgency,
};
pub use power::{PowerEvent, PowerEvents, PowerSource, PowerState, PowerStatus, PowerWatcher};
pub use recent::RecentDocuments;
pub use sleep::SleepBlocker;
