mod emitter;
mod events;
pub(crate) mod frame;
mod request_dispatch;
mod writer;

pub use emitter::BridgeEventEmitter;
pub(crate) use events::{
    parse_host_control, platform_event_payload, prepare_bridge_command, spawn_bridge_dispatch,
    spawn_bridge_dispatch_for_window,
};
