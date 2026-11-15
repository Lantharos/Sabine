mod bridge;
mod clipboard;
mod desktop;
mod diagnostics;
mod error;
mod host;
mod launch;
mod media;
mod menu;
mod osr;
mod render;
mod storage;
mod window;

pub use bridge::{BridgeEventEmitter, DialogAnswer, DialogInput, DialogReply, MessageDialog};
pub use desktop::TrayHandle;
pub use diagnostics::{CrashReport, CrashReports, Logging};
pub use error::{SabineError, SabineResult};
pub use host::{SabineProcess, SabineProcessHandle, WindowId};
#[cfg(target_os = "linux")]
pub use launch::confine::documents;
pub use launch::metrics::{SabineLaunchMetric, SabineLaunchMetricsSnapshot};
pub use log;
pub use menu::{MenuActivation, MenuItem, MenuKind, MenuRole};
pub use sabine_bridge::{
    ActivityOptions, ActivityRecord, BridgeCommand, BridgeCommandDescriptor, BridgeError,
    BridgeEvent, BridgeResponse, BridgeResult, Bytes, ContentSecurity, GuestBounds,
    GuestCreateOptions, GuestDownloadAction, GuestHostControl, GuestPopupPolicy, KeepAwake,
    SabineActivityLease, TypedRequest, WindowControl, WindowSize, WithBody,
};
pub use sabine_platform::{
    AppBadge, AppProgress, Appearance, AutostartEntry, BackdropMaterial, BackdropPreference,
    BackdropRole, Display, DisplayRect, GlobalShortcutActivation, GlobalShortcutFailure,
    GlobalShortcutRegistration, IdleState, IdleWatch, NativeMessagingHost, Notification,
    NotificationAction, NotificationEvent, PlatformEvent, PowerEvent, PowerSource, PowerState,
    Shortcut, ShortcutModifiers, SingleInstanceActivation, SingleInstancePolicy, SleepBlocker,
    TrayActivation, TrayIcon, TrayMenuItem, TrayMenuItemKind, Urgency, WindowBackgroundEffect,
    WindowRegion, WindowRegionRect, WindowRegions, async_runtime,
};
pub use sabine_runtime::{RuntimeConfig, RuntimeMode};
pub use sabine_service::{AppEnvironment, Permission};
pub use storage::{Secrets, Store, StoreChange};
pub use window::{
    AppChrome, Download, DownloadControl, DownloadDecision, DownloadState, PermissionDecision,
    PermissionRequest, SabineColor, SabineLifecyclePolicy, SabineWindow, SabineWindowChrome,
    SabineWindowControlAction, SabineWindowControlRegion, Spellcheck, WindowVisibility,
    resource_path,
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
pub fn dispatch_host_mode_from_args(args: &[std::ffi::OsString]) -> bool {
    launch::dispatch_host_mode_from_args(args)
}
