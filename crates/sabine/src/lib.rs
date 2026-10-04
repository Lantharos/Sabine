pub use sabine_service::AppEnvironment;
mod bridge;
#[cfg(target_os = "linux")]
mod clipboard;
mod desktop;
mod error;
mod host;
mod launch;
mod media;
mod osr;
mod render;
mod window;

pub use desktop::TrayHandle;
pub use error::{SabineError, SabineResult};
pub use host::{SabineProcess, SabineProcessHandle, WindowId};
pub use window::{
    AppChrome, SabineColor, SabineLifecyclePolicy, SabineWindow, SabineWindowChrome,
    SabineWindowControlAction, WindowVisibility,
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

pub use bridge::BridgeEventEmitter;
pub use window::SabineWindowControlRegion;

pub use launch::metrics::{SabineLaunchMetric, SabineLaunchMetricsSnapshot};
pub use sabine_bridge::{ActivityOptions, ActivityRecord, SabineActivityLease};
pub use sabine_bridge::{
    BridgeCommand, BridgeCommandDescriptor, BridgeError, BridgeResponse, BridgeResult,
    ContentSecurity,
};
pub use sabine_bridge::{
    GuestBounds, GuestCreateOptions, GuestDownloadAction, GuestHostControl, GuestPopupPolicy,
};
pub use sabine_platform::{
    AutostartEntry, GlobalShortcutFailure, GlobalShortcutRegistration, NativeMessagingHost,
    PlatformEvent, Shortcut, ShortcutModifiers, SingleInstancePolicy, TrayActivation, TrayIcon,
    TrayMenuItem, TrayMenuItemKind, WindowBackgroundEffect, WindowRegion, WindowRegionRect,
    WindowRegions,
};
pub use sabine_runtime::{RuntimeConfig, RuntimeMode};

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

pub(crate) use bridge::{
    parse_host_control, prepare_bridge_command, spawn_bridge_dispatch,
    spawn_bridge_dispatch_for_window,
};
pub(crate) use launch::{apply_browser_launch_args, centered_window_position};
