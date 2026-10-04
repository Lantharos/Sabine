use base64::{Engine, engine::general_purpose::STANDARD};
use sabine_bridge::clipboard::{READ_COMMAND, WRITE_COMMAND};
use serde::Deserialize;
use serde_json::{Value, json};

use super::BridgeRequest;
#[cfg(not(target_os = "linux"))]
use crate::clipboard::SystemClipboard;
use crate::clipboard::{ClipboardContent, Selection};
use crate::osr::host::events::bridge_response_line;
use crate::osr::host::native::OsrNativeHost;

#[derive(Deserialize)]
struct ReadRequest {
    selection: Selection,
    types: Option<Vec<String>>,
    trusted: bool,
}

#[derive(Deserialize)]
struct WriteRequest {
    selection: Selection,
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    #[serde(rename = "type")]
    mime: String,
    text: Option<String>,
    base64: Option<String>,
}

impl OsrNativeHost {
    /// Connects before the browser starts, so its pages know they can reach
    /// the clipboard.
    #[cfg(not(target_os = "linux"))]
    pub(in crate::osr::host) fn connect_clipboard(&mut self) {
        if self.clipboard.is_none() {
            self.clipboard = Some(SystemClipboard::connect());
            self.config.bridge_policy["clipboard"] = true.into();
        }
    }

    /// Chromium pastes by itself here, so only the app's own pages read the
    /// clipboard through the window.
    #[cfg(not(target_os = "linux"))]
    fn pasting(&self) -> bool {
        false
    }

    pub(super) fn answer_clipboard(&mut self, request: &BridgeRequest) {
        match request.command {
            READ_COMMAND => self.read_clipboard(request),
            WRITE_COMMAND => {
                let result = self.write_clipboard(request.payload);
                self.send_bridge_response(
                    request.browser_id,
                    request.request_id,
                    result.map(|()| Value::Null),
                );
            }
            _ => self.send_bridge_response(
                request.browser_id,
                request.request_id,
                Err(format!("{} is not a clipboard command", request.command)),
            ),
        }
    }

    fn read_clipboard(&mut self, request: &BridgeRequest) {
        let read = match serde_json::from_str::<ReadRequest>(request.payload) {
            Ok(read) => read,
            Err(error) => {
                self.send_bridge_response(
                    request.browser_id,
                    request.request_id,
                    Err(error.to_string()),
                );
                return;
            }
        };
        let (Some(clipboard), true) = (&self.clipboard, read.trusted || self.pasting()) else {
            self.send_bridge_response(
                request.browser_id,
                request.request_id,
                Err("The clipboard can only be read when pasting".to_string()),
            );
            return;
        };
        let writer = self.control_writer.clone();
        let browser_id = request.browser_id.to_string();
        let request_id = request.request_id.to_string();
        clipboard.read(
            read.selection,
            read.types,
            Box::new(move |content| {
                if let Some(writer) = writer {
                    let payload = content.map(|content| items_json(&content));
                    let _ = writer.send(bridge_response_line(&browser_id, &request_id, payload));
                }
            }),
        );
    }

    fn write_clipboard(&self, payload: &str) -> Result<(), String> {
        let write =
            serde_json::from_str::<WriteRequest>(payload).map_err(|error| error.to_string())?;
        let clipboard = self
            .clipboard
            .as_ref()
            .ok_or_else(|| "The clipboard is unavailable".to_string())?;
        let mut content = ClipboardContent::default();
        for item in write.items {
            let bytes = match (item.text, item.base64) {
                (Some(text), _) => text.into_bytes(),
                (None, Some(encoded)) => STANDARD
                    .decode(encoded)
                    .map_err(|error| error.to_string())?,
                (None, None) => return Err(format!("{} has no data", item.mime)),
            };
            content.push(item.mime, bytes);
        }
        if !content.is_empty() {
            clipboard.write(write.selection, content)?;
        }
        Ok(())
    }
}

fn items_json(content: &ClipboardContent) -> Value {
    content
        .items()
        .map(|(mime, bytes)| {
            if mime.starts_with("text/") {
                json!({ "type": mime, "text": String::from_utf8_lossy(bytes) })
            } else {
                json!({ "type": mime, "base64": STANDARD.encode(bytes) })
            }
        })
        .collect()
}
