use serde_json::Value;

use super::BridgeRequest;
use crate::osr::host::native::OsrNativeHost;

impl OsrNativeHost {
    /// Linux makes the token from the window's keyboard focus. Other
    /// desktops let the app in front hand focus on without one.
    pub(super) fn answer_activation_token(&self, request: &BridgeRequest) {
        #[cfg(target_os = "linux")]
        if let (Some(clipboard), Some(writer)) = (&self.clipboard, self.control_writer.clone()) {
            let browser_id = request.browser_id.to_string();
            let request_id = request.request_id.to_string();
            clipboard.activation_token(Box::new(move |token| {
                let line = crate::osr::host::events::bridge_response_line(
                    &browser_id,
                    &request_id,
                    Ok(token.map_or(Value::Null, Value::String)),
                );
                let _ = writer.send(line);
            }));
            return;
        }
        self.send_bridge_response(request.browser_id, request.request_id, Ok(Value::Null));
    }
}
