//! Sabine bridge, activity, guest, and web page IPC primitives.
//!
//! `sabine-bridge` is the load-bearing crate for the bridge protocol that
//! lets a Sabine page call into the native host. The wire format, JavaScript
//! surface, activity registry, and security model have one source of truth.
//!
//! Crates that depend on `sabine-bridge`:
//!
//! - `sabine` — drives the C++ CEF OSR host and re-exports these types.
//! - Apps depend on `sabine` (which re-exports the bridge surface).

mod activity;
mod bridge;
mod guest;
mod guest_create;
mod guest_download;
mod guest_host_control;
pub mod media;
mod metrics;

pub use activity::{
    ActivityEventEmitter, ActivityHostUpdate, ActivityOptions, ActivityRecord, ActivityRegistry,
    SabineActivityLease, bridge_commands_with_all_internal, host_update_json,
};
pub use bridge::{
    BridgeCommand, BridgeCommandDescriptor, BridgeError, BridgeHandlers, BridgeRegistry,
    BridgeResponse, BridgeResult, BridgeRuntime, ContentSecurity,
};
pub use guest::{
    GuestBounds, GuestCreateOptions, GuestDownloadAction, GuestDownloadEvent, GuestDownloadState,
    GuestHostControl, GuestInfo, GuestPopupPolicy,
};
pub use metrics::{
    LaunchMetrics, SABINE_TRACE_ENV, SabineLaunchMetric, SabineLaunchMetricsSnapshot,
};

/// The `window.sabine` page API injected into every Sabine page by the native host.
pub const INSTALL_SCRIPT: &str = include_str!("web_bridge.js");
