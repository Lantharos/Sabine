#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
compile_error!("Sabine supports Linux, macOS, and Windows desktop targets");

mod desktop;
#[cfg(target_os = "linux")]
mod linux;
mod shortcut_inhibit;
mod tasks;
mod window;

pub use desktop::{
    Appearance, AppearanceWatcher, AutostartEntry, DeepLinkRegistration, GlobalShortcutActivation,
    GlobalShortcutFailure, GlobalShortcutRegistration, NativeMessagingHost, Notification,
    NotificationEvent, NotificationEvents, Notifier, PlatformEvent, Shortcut, ShortcutModifiers,
    SingleInstanceActivation, SingleInstancePolicy, TrayActivation, TrayIcon, TrayMenuItem,
    TrayMenuItemKind, Urgency, shell,
};
#[cfg(target_os = "linux")]
pub use linux::portal;
pub use shortcut_inhibit::ShortcutInhibitor;
#[cfg(target_os = "windows")]
pub use shortcut_inhibit::SystemKey;
pub use tasks::async_runtime;
#[cfg(target_os = "macos")]
pub use window::{ControlsOverlay, controls_overlay};
pub use window::{
    WindowBackgroundEffect, WindowChrome, WindowEffect, WindowOptions, WindowRegion,
    WindowRegionAdaptive, WindowRegionRect, WindowRegions,
};
#[cfg(target_os = "linux")]
pub use window::{WindowPlacement, resizes_from_outside};
