mod bridge;
mod clipboard;
mod desktop;
mod error;
mod host;
mod launch;
mod media;
mod osr;
mod render;
mod window;

pub use bridge::BridgeEventEmitter;
pub use desktop::TrayHandle;
pub use error::{SabineError, SabineResult};
pub use host::{SabineProcess, SabineProcessHandle, WindowId};
#[cfg(target_os = "linux")]
pub use launch::confine::documents;
pub use launch::metrics::{SabineLaunchMetric, SabineLaunchMetricsSnapshot};
pub use sabine_bridge::{
    ActivityOptions, ActivityRecord, BridgeCommand, BridgeCommandDescriptor, BridgeError,
    BridgeEvent, BridgeResponse, BridgeResult, Bytes, ContentSecurity, GuestBounds,
    GuestCreateOptions, GuestDownloadAction, GuestHostControl, GuestPopupPolicy,
    SabineActivityLease,
};
pub use sabine_platform::{
    Appearance, AutostartEntry, GlobalShortcutActivation, GlobalShortcutFailure,
    GlobalShortcutRegistration, NativeMessagingHost, Notification, PlatformEvent, Shortcut,
    ShortcutModifiers, SingleInstanceActivation, SingleInstancePolicy, TrayActivation, TrayIcon,
    TrayMenuItem, TrayMenuItemKind, Urgency, WindowBackgroundEffect, WindowRegion,
    WindowRegionRect, WindowRegions, async_runtime,
};
pub use sabine_runtime::{RuntimeConfig, RuntimeMode};
pub use sabine_service::AppEnvironment;
pub use window::{
    AppChrome, SabineColor, SabineLifecyclePolicy, SabineWindow, SabineWindowChrome,
    SabineWindowControlAction, SabineWindowControlRegion, WindowVisibility, resource_path,
};

/// Common imports for app authors.
pub mod prelude {
    pub use crate::{
        AppChrome, AppEnvironment, BridgeCommand, BridgeError, BridgeResponse, BridgeResult,
        SabineColor, SabineError, SabineLifecyclePolicy, SabineProcess, SabineProcessHandle,
        SabineResult, SabineWindow, SabineWindowChrome, TrayIcon, WindowBackgroundEffect,
        WindowRegion, WindowRegionRect,
    };
}

/// Runs an internal Sabine child mode selected by `args`.
///
/// Custom entry points should call this before argument parsers, logging,
/// configuration, or other application initialization and return immediately
/// when it yields `true`. The dispatcher recognizes both OSR-host and runtime
/// bootstrap children. Ordinary application arguments return `false` without
/// initializing Sabine.
pub fn dispatch_host_mode_from_args(args: &[String]) -> bool {
    launch::dispatch_host_mode_from_args(args)
}
