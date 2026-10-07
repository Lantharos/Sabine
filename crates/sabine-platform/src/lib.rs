mod appearance;
#[cfg(target_os = "macos")]
mod controls_overlay;
mod desktop_integration;
mod effect;
mod notifications;
mod regions;
mod shortcut_inhibit;
#[cfg(target_os = "linux")]
mod wayland_client;
mod window_options;
#[cfg(target_os = "linux")]
mod window_placement;

pub use appearance::Appearance;
#[cfg(target_os = "linux")]
pub use appearance::AppearanceWatcher;
#[cfg(not(target_os = "linux"))]
pub use appearance::system_appearance;
#[cfg(target_os = "macos")]
pub use controls_overlay::{ControlsOverlay, controls_overlay};
pub use desktop_integration::{
    AutostartEntry, DeepLinkRegistration, GlobalShortcutActivation, GlobalShortcutFailure,
    GlobalShortcutRegistration, NativeMessagingHost, PlatformEvent, Shortcut, ShortcutModifiers,
    SingleInstanceActivation, SingleInstancePolicy, TrayActivation, TrayIcon, TrayMenuItem,
    TrayMenuItemKind,
};
pub use effect::WindowEffect;
pub use notifications::{Notification, NotificationEvent, NotificationEvents, Notifier};
pub use regions::{WindowRegion, WindowRegionAdaptive, WindowRegionRect, WindowRegions};
pub use shortcut_inhibit::ShortcutInhibitor;
#[cfg(target_os = "windows")]
pub use shortcut_inhibit::SystemKey;
pub use window_options::{
    PlatformOs, WindowBackgroundEffect, WindowChrome, WindowOptions, current_desktop_os,
};
#[cfg(target_os = "linux")]
pub use window_placement::WindowPlacement;
