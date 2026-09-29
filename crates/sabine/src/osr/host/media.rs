use std::sync::Arc;

use serde_json::{Value, json};

use crate::media::{Events, Rect, SourcePolicy, Viewport};
use crate::osr::control::ControlWriter;

use super::native::OsrNativeHost;

impl OsrNativeHost {
    /// Answers the page's `sabine.media.*` requests here, where the window
    /// lives; any other bridge request is left for the app.
    pub(super) fn handle_media_request(&mut self, line: &str) -> bool {
        let mut parts = line.splitn(6, '\t');
        let (
            Some("SABINE_BRIDGE_REQUEST"),
            Some(browser),
            Some(request),
            Some(_),
            Some(command),
            Some(payload),
        ) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        )
        else {
            return false;
        };
        if !command.starts_with(sabine_bridge::media::COMMAND_PREFIX) {
            return false;
        }
        let params = serde_json::from_str(payload).unwrap_or(Value::Null);
        let policy = SourcePolicy {
            web_root: self.config.web_root.as_deref(),
            local_files: self.config.local_files,
        };
        let writer = self.control_writer.clone();
        let (status, payload) = match self
            .media
            .handle(command, params, &policy, |id| media_events(writer, id))
        {
            Ok(result) => ("ok", result),
            Err(message) => ("error", json!({ "message": message })),
        };
        self.send_control(&format!(
            "SABINE_BRIDGE_RESPONSE\t{browser}\t{request}\t{status}\t{payload}\n"
        ));
        self.sync_media();
        true
    }

    pub(super) fn media_viewport(&self) -> Viewport {
        let top = f64::from(self.titlebar_height());
        let scale = self
            .window
            .as_ref()
            .map_or(self.scale_factor, |window| window.scale_factor());
        Viewport {
            content: Rect::new(
                0.0,
                top,
                f64::from(self.logical_width()),
                (f64::from(self.logical_height()) - top).max(0.0),
            ),
            scale,
        }
    }

    pub(super) fn relayout_media(&mut self) {
        let viewport = self.media_viewport();
        self.media.set_viewport(viewport);
        self.sync_media();
    }

    pub(super) fn clear_media(&mut self) {
        self.media.clear();
        self.sync_media();
    }

    /// Re-presents the window after the page's media holes moved.
    pub(super) fn sync_media(&mut self) {
        if self.media.take_changed() {
            self.update_effect_regions();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

fn media_events(writer: Option<Arc<ControlWriter>>, id: u64) -> Events {
    Arc::new(move |mut payload: Value| {
        payload["id"] = id.into();
        if let Some(writer) = &writer {
            let _ = writer.send(format!(
                "SABINE_BRIDGE_EVENT\t\"{}\"\t{payload}\n",
                sabine_bridge::media::EVENT
            ));
        }
    })
}
