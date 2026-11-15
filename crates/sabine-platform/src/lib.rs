#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
compile_error!("Sabine supports Linux, macOS, and Windows desktop targets");

mod desktop;
mod display;
#[cfg(target_os = "linux")]
mod linux;
mod shortcut_inhibit;
mod tasks;
mod window;

pub use desktop::{
    AppBadge, AppProgress, Appearance, AppearanceWatcher, AutostartEntry, DeepLinkRegistration,
    GlobalShortcutActivation, GlobalShortcutFailure, GlobalShortcutRegistration,
    NativeMessagingHost, Notification, NotificationEvent, NotificationEvents, Notifier,
    PlatformEvent, RecentDocuments, Shortcut, ShortcutModifiers, SingleInstanceActivation,
    SingleInstancePolicy, TrayActivation, TrayIcon, TrayMenuItem, TrayMenuItemKind, Urgency,
    secrets, shell,
};
pub use display::{Display, DisplayRect, DisplayWatcher, displays};
#[cfg(target_os = "linux")]
pub use linux::portal;
pub use shortcut_inhibit::ShortcutInhibitor;
#[cfg(target_os = "windows")]
pub use shortcut_inhibit::SystemKey;
pub use tasks::async_runtime;
#[cfg(target_os = "windows")]
pub use window::Taskbar;
pub use window::{
    BackdropMaterial, BackdropPreference, BackdropRole, WindowBackdrop, WindowBackgroundEffect,
    WindowChrome, WindowEffect, WindowOptions, WindowRegion, WindowRegionAdaptive,
    WindowRegionRect, WindowRegions,
};
#[cfg(target_os = "linux")]
pub use window::{CompositorSupport, WindowPlacement, compositor_support};
#[cfg(target_os = "macos")]
pub use window::{ControlsOverlay, controls_overlay};
