use sabine_bridge::clipboard::{READ_COMMAND, WRITE_COMMAND};
use serde::Deserialize;
use serde_json::{Value, json};

use super::BridgeRequest;
#[cfg(not(target_os = "linux"))]
use crate::clipboard::SystemClipboard;
use crate::clipboard::{ClipboardContent, MAX_READ_BYTES, Selection};
use crate::osr::host::events::{bridge_bytes_response, bridge_response_line};
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
    #[cfg(target_os = "linux")]
    trusted: bool,
}

/// One item of the bytes a request carries: its type and how many of the
/// bytes, in order, are its own.
#[derive(Deserialize)]
struct Item {
    #[serde(rename = "type")]
    mime: String,
    size: usize,
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
        if request.command == READ_COMMAND {
            self.read_clipboard(request);
            return;
        }
        self.send_bridge_response(
            request.browser_id,
            request.request_id,
            Err(format!("{} is not a clipboard command", request.command)),
        );
    }

    /// Writes what a page copied, whose bytes ride with its request. Returns
    /// false for every other request carrying bytes, so it reaches the app.
    pub(in crate::osr::host) fn answer_clipboard_write(&mut self, line: &str, body: &[u8]) -> bool {
        let Some(request) =
            BridgeRequest::parse(line).filter(|request| request.command == WRITE_COMMAND)
        else {
            return false;
        };
        let result = self.write_clipboard(request.payload, body);
        self.send_bridge_response(
            request.browser_id,
            request.request_id,
            result.map(|()| Value::Null),
        );
        true
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
                let Some(writer) = writer else {
                    return;
                };
                let sent = match content.and_then(|content| packed(&content)) {
                    Ok((items, body)) => writer.send_bytes(bridge_bytes_response(
                        &browser_id,
                        &request_id,
                        &items,
                        &body,
                    )),
                    Err(error) => {
                        writer.send(bridge_response_line(&browser_id, &request_id, Err(error)))
                    }
                };
                if let Err(error) = sent {
                    sabine_runtime::report_error("clipboard", error);
                }
            }),
        );
    }

    fn write_clipboard(&self, payload: &str, body: &[u8]) -> Result<(), String> {
        let write =
            serde_json::from_str::<WriteRequest>(payload).map_err(|error| error.to_string())?;
        let clipboard = self
            .clipboard
            .as_ref()
            .ok_or_else(|| "The clipboard is unavailable".to_string())?;
        let mut content = ClipboardContent::default();
        let mut rest = body;
        for item in write.items {
            let (bytes, after) = rest
                .split_at_checked(item.size)
                .ok_or_else(|| format!("{} is missing its data", item.mime))?;
            content.push(item.mime, bytes);
            rest = after;
        }
        #[cfg(target_os = "linux")]
        if write.trusted {
            content.share_files();
        }
        if !content.is_empty() {
            clipboard.write(write.selection, content)?;
        }
        Ok(())
    }
}

/// The items' types and sizes, and their bytes one after another.
fn packed(content: &ClipboardContent) -> Result<(Value, Vec<u8>), String> {
    let size = content.items().map(|(_, bytes)| bytes.len()).sum::<usize>();
    if size > MAX_READ_BYTES {
        return Err("The clipboard holds more than 32 MiB".to_string());
    }
    let mut body = Vec::with_capacity(size);
    let items = content
        .items()
        .map(|(mime, bytes)| {
            body.extend_from_slice(bytes);
            json!({ "type": mime, "size": bytes.len() })
        })
        .collect();
    Ok((items, body))
}
