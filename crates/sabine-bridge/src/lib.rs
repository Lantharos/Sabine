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

mod bridge;
mod commands;
mod guest;

pub use bridge::{
    BridgeCommand, BridgeCommandDescriptor, BridgeError, BridgeFuture, BridgeHandlers,
    BridgeOutcome, BridgeRegistry, BridgeResponse, BridgeResult, BridgeRuntime, ContentSecurity,
};
pub use commands::activity::{
    ActivityEventEmitter, ActivityHostUpdate, ActivityOptions, ActivityRecord, ActivityRegistry,
    SabineActivityLease, host_update_json,
};
pub use commands::{
    APPEARANCE_COMMAND, APPEARANCE_EVENT, CONTROLS_OVERLAY_COMMAND, CONTROLS_OVERLAY_EVENT,
    INHIBIT_SHORTCUTS_COMMAND, SET_REGIONS_COMMAND, clipboard, media, page_commands,
};
pub use guest::{
    GuestBounds, GuestCreateOptions, GuestDownloadAction, GuestHostControl, GuestPopupPolicy,
};

/// The `window.sabine` page API injected into every Sabine page by the native host.
pub const INSTALL_SCRIPT: &str = include_str!("scripts/web_bridge.js");
