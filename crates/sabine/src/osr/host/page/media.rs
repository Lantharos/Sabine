use std::sync::Arc;

use serde_json::Value;

use crate::media::{Events, Rect, SourcePolicy, Viewport};
use crate::osr::control::ControlWriter;

use super::BridgeRequest;
use crate::osr::host::native::OsrNativeHost;

impl OsrNativeHost {
    pub(super) fn answer_media(&mut self, request: &BridgeRequest) {
        let params = serde_json::from_str(request.payload).unwrap_or(Value::Null);
        let policy = SourcePolicy {
            web_root: self.config.web_root.as_deref(),
            local_files: self.config.local_files,
        };
        let writer = self.control_writer.clone();
        let result = self.media.handle(request.command, params, &policy, |id| {
            media_events(writer, id)
        });
        self.send_bridge_response(request.browser_id, request.request_id, result);
        self.sync_media();
    }

    pub(in crate::osr::host) fn media_viewport(&self) -> Viewport {
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

    pub(in crate::osr::host) fn relayout_media(&mut self) {
        let viewport = self.media_viewport();
        self.media.set_viewport(viewport);
        self.sync_media();
    }

    pub(in crate::osr::host) fn clear_media(&mut self) {
        self.media.clear();
        self.sync_media();
    }

    /// Re-presents the window after the page's media holes moved.
    pub(in crate::osr::host) fn sync_media(&mut self) {
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
