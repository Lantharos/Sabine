use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use sabine_bridge::clipboard::{READ_COMMAND, WRITE_COMMAND};
use serde::Deserialize;
use serde_json::{Value, json};
use winit::{
    event::KeyEvent,
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, NamedKey, PhysicalKey},
    window::Window,
};

use super::BridgeRequest;
use crate::clipboard::{ClipboardContent, Selection, SystemClipboard};
use crate::osr::host::events::bridge_response_line;
use crate::osr::host::native::OsrNativeHost;

/// How long after the user asks to paste a page outside the app may read
/// the clipboard.
const PASTE_GESTURE: Duration = Duration::from_secs(2);

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
    pub(in crate::osr::host) fn connect_clipboard(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.clipboard.is_some() {
            return;
        }
        let proxy = self.proxy.clone();
        match SystemClipboard::connect(
            event_loop.rwh_06_handle(),
            Arc::new(move || proxy.wake_up()),
        ) {
            Ok(clipboard) => {
                self.clipboard = Some(clipboard);
                self.config.bridge_policy["clipboard"] = true.into();
            }
            Err(error) => eprintln!("Sabine clipboard: {error}"),
        }
    }

    pub(in crate::osr::host) fn attach_clipboard(&self, window: &dyn Window) {
        if let Some(clipboard) = &self.clipboard {
            clipboard.attach(window);
        }
    }

    pub(in crate::osr::host) fn note_paste_key(&mut self, event: &KeyEvent) {
        let control = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        let paste = (control && event.physical_key == PhysicalKey::Code(KeyCode::KeyV))
            || (shift && event.logical_key == NamedKey::Insert);
        if paste {
            self.paste_gesture = Some(Instant::now());
        }
    }

    pub(in crate::osr::host) fn note_middle_click(&mut self) {
        self.paste_gesture = Some(Instant::now());
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
        let pasting = self
            .paste_gesture
            .is_some_and(|at| at.elapsed() < PASTE_GESTURE);
        let (Some(clipboard), true) = (&self.clipboard, read.trusted || pasting) else {
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
            clipboard.write(write.selection, content);
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
