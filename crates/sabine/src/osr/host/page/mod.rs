mod clipboard;
mod media;
mod notifications;
#[cfg(target_os = "linux")]
mod paste_gesture;

use sabine_bridge::{
    APPEARANCE_COMMAND, CONTROLS_OVERLAY_COMMAND, INHIBIT_SHORTCUTS_COMMAND, SET_REGIONS_COMMAND,
};

use super::native::OsrNativeHost;

/// A page's bridge call, as the browser host forwards it.
pub(super) struct BridgeRequest<'a> {
    pub(super) browser_id: &'a str,
    pub(super) request_id: &'a str,
    pub(super) command: &'a str,
    pub(super) payload: &'a str,
}

impl<'a> BridgeRequest<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        let parts = line.splitn(6, '\t').collect::<Vec<_>>();
        let [
            "SABINE_BRIDGE_REQUEST",
            browser_id,
            request_id,
            _,
            command,
            payload,
        ] = parts[..]
        else {
            return None;
        };
        Some(Self {
            browser_id,
            request_id,
            command,
            payload,
        })
    }
}

impl OsrNativeHost {
    fn answer_set_regions(&mut self, request: &BridgeRequest) {
        let result = serde_json::from_str::<serde_json::Value>(request.payload)
            .map(|regions| {
                self.set_regions(crate::osr::protocol::regions_from_json(Some(&regions)))
            })
            .map(|()| serde_json::Value::Null)
            .map_err(|error| error.to_string());
        self.send_bridge_response(request.browser_id, request.request_id, result);
    }

    /// Answers the bridge commands that act on this window. Returns false for
    /// every other command so it reaches the app.
    pub(super) fn answer_window_bridge_request(&mut self, line: &str) -> bool {
        let Some(request) = BridgeRequest::parse(line) else {
            return false;
        };
        match request.command {
            INHIBIT_SHORTCUTS_COMMAND => self.answer_inhibit_shortcuts(&request),
            SET_REGIONS_COMMAND => self.answer_set_regions(&request),
            APPEARANCE_COMMAND => self.send_bridge_response(
                request.browser_id,
                request.request_id,
                Ok(self.appearance_json()),
            ),
            CONTROLS_OVERLAY_COMMAND => self.send_bridge_response(
                request.browser_id,
                request.request_id,
                Ok(self.controls_overlay()),
            ),
            command if command.starts_with(sabine_bridge::media::COMMAND_PREFIX) => {
                self.answer_media(&request)
            }
            command if command.starts_with(sabine_bridge::NOTIFICATION_COMMAND_PREFIX) => {
                self.answer_notification(&request)
            }
            command if command.starts_with(sabine_bridge::clipboard::COMMAND_PREFIX) => {
                self.answer_clipboard(&request)
            }
            _ => return false,
        }
        true
    }
}
