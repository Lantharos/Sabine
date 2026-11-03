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

mod bindings;
mod bridge;
mod commands;
mod guest;

pub use bindings::{BridgeBindings, BridgeEvent, Bytes};
pub use bridge::{
    BridgeCommand, BridgeCommandDescriptor, BridgeError, BridgeFuture, BridgeHandlers,
    BridgeOutcome, BridgeRegistry, BridgeResponse, BridgeResult, BridgeRuntime, ContentSecurity,
};
pub use commands::activity::{
    ActivityEventEmitter, ActivityHostUpdate, ActivityOptions, ActivityRecord, ActivityRegistry,
    SabineActivityLease, host_update_json,
};
pub use commands::{
    ACTIVATION_TOKEN_COMMAND, APPEARANCE_COMMAND, APPEARANCE_EVENT, CONTROLS_OVERLAY_COMMAND,
    CONTROLS_OVERLAY_EVENT, DRAG_FILES_COMMAND, HOLD_REVEAL_COMMAND, INHIBIT_SHORTCUTS_COMMAND,
    NOTIFICATION_CLOSE_COMMAND, NOTIFICATION_COMMAND_PREFIX, NOTIFICATION_SHOW_COMMAND,
    OPEN_LINK_COMMAND, PAGE_ZOOM_COMMAND, PRINT_FILES_COMMAND, REVEAL_COMMAND, SET_REGIONS_COMMAND,
    SHOW_FOLDER_COMMAND, clipboard, media, page_commands,
};
pub use guest::{
    GuestBounds, GuestCreateOptions, GuestDownloadAction, GuestHostControl, GuestPopupPolicy,
};

/// The `window.sabine` page API injected into every Sabine page by the native host. Its parts
/// share one function scope, and every part after the first uses what `core.js` declares.
pub const INSTALL_SCRIPT: &str = concat!(
    "(function () {\nif (window.sabine?.bridge?.__native) return;\n",
    include_str!("scripts/bridge/core.js"),
    include_str!("scripts/bridge/window.js"),
    include_str!("scripts/bridge/print.js"),
    include_str!("scripts/bridge/notifications.js"),
    include_str!("scripts/bridge/drag.js"),
    include_str!("scripts/bridge/activity.js"),
    include_str!("scripts/bridge/guest.js"),
    "})();\n",
);

/// Middle-click autoscroll, evaluated in every frame by the native host on Linux and macOS, where
/// Chromium has none.
pub const AUTOSCROLL_SCRIPT: &str = include_str!("scripts/autoscroll.js");

/// Reports the text around the caret of the focused editable to the input method, evaluated in
/// every frame by the native host with the reporting function it alone holds.
pub const IME_STATE_SCRIPT: &str = include_str!("scripts/ime_state.js");

/// Tells the native host each time the main frame's resolution changes, so the window can follow
/// the page's zoom, evaluated by the native host with the reporting function it alone holds.
pub const PAGE_ZOOM_SCRIPT: &str = include_str!("scripts/page_zoom.js");

/// Turns the back and forward mouse buttons into history navigation unless the page handles
/// them, called by the native host with the pointer position, button, and modifiers.
pub const MOUSE_NAVIGATION_SCRIPT: &str = include_str!("scripts/mouse_navigation.js");

/// Clears the page background of transparent windows, evaluated by the native host as the main
/// document starts and finishes loading.
pub const TRANSPARENT_BACKGROUND_SCRIPT: &str = include_str!("scripts/transparent_background.js");
