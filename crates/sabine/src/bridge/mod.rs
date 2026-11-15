mod bindings;
mod emitter;
mod events;
pub(crate) mod frame;
mod listeners;
mod request_dispatch;
#[cfg(target_os = "windows")]
mod taskbar;
mod writer;

pub(crate) use bindings::export_requested_bindings;
pub use emitter::BridgeEventEmitter;
pub(crate) use events::{
    FOCUSED_LINE, parse_host_control, platform_event_payload,
    prepare_bridge_command, spawn_bridge_dispatch,
};
