use std::io::Write;
use std::sync::Arc;

use crate::bridge::frame::Frame;
use crate::osr::host::native::OsrNativeHost;
use crate::render::ImageId;

impl OsrNativeHost {
    /// Answers the page's bridge requests the window can answer itself, and
    /// passes the rest on to the app.
    pub(super) fn relay_bridge_request(&mut self, frame: Frame) {
        let answered = frame.body.is_none() && self.answer_window_bridge_request(&frame.line);
        if !frame.line.is_empty() && !answered {
            let mut output = std::io::stdout();
            let _ = output.write_all(&frame.to_bytes());
            let _ = output.flush();
        }
    }

    pub(super) fn capture_guest(&self, browser_id: &str, request_id: &str, guest_id: &str) {
        let result = self
            .renderer
            .as_ref()
            .zip(self.overlays.get_key_value(guest_id))
            .and_then(|(renderer, (id, _))| {
                renderer.read_bgra_image(&ImageId::Overlay(Arc::clone(id)))
            })
            .ok_or_else(|| "guest has no frame to capture".to_string())
            .and_then(|frame| {
                crate::osr::host::paint::guest_preview::guest_preview_data_url(
                    &frame.bytes,
                    frame.width,
                    frame.height,
                )
            });
        self.send_bridge_response(
            browser_id,
            request_id,
            result.map(|data_url| serde_json::json!({ "dataUrl": data_url })),
        );
    }

    pub(in crate::osr::host) fn send_bridge_response(
        &self,
        browser_id: &str,
        request_id: &str,
        result: Result<serde_json::Value, String>,
    ) {
        self.send_control(bridge_response_line(browser_id, request_id, result));
    }
}

pub(in crate::osr::host) fn bridge_response_line(
    browser_id: &str,
    request_id: &str,
    result: Result<serde_json::Value, String>,
) -> String {
    let (status, payload) = match result {
        Ok(payload) => ("ok", payload),
        Err(message) => ("error", serde_json::json!({ "message": message })),
    };
    format!("SABINE_BRIDGE_RESPONSE\t{browser_id}\t{request_id}\t{status}\t{payload}\n")
}
