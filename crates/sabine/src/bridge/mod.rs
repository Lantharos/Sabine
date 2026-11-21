mod bindings;
mod emitter;
mod events;
pub(crate) mod frame;
mod listeners;
mod request_dispatch;
mod writer;

pub(crate) use bindings::export_requested_bindings;
pub use emitter::BridgeEventEmitter;
pub(crate) use emitter::{APP_MENU_LINE, DIALOG_ANSWER_LINE};
pub use emitter::{DialogAnswer, DialogInput, DialogReply, MessageDialog};
#[cfg(target_os = "linux")]
pub(crate) use events::CLOSING_LINE;
pub(crate) use events::{
    FOCUSED_LINE, QUIT_REQUESTED_LINE, parse_host_control, platform_event_payload,
    power_source_json, prepare_bridge_command, spawn_bridge_dispatch,
};
