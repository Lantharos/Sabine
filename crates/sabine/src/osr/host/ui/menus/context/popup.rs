use serde::Deserialize;

use super::MenuOrigin;
use super::items::app_items;
use crate::MenuItem;
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::page::BridgeRequest;

/// A menu a page shows at a point of its own, in CSS pixels, as for a
/// button it draws.
#[derive(Deserialize)]
struct Popup {
    items: Vec<MenuItem>,
    x: f32,
    y: f32,
}

impl OsrNativeHost {
    /// Shows a page's menu, answering with the id of the item chosen, or
    /// `null` once the menu closes without a choice.
    pub(in crate::osr::host) fn open_popup_menu(&mut self, request: &BridgeRequest) {
        let popup = serde_json::from_str::<Popup>(request.payload)
            .map_err(|error| error.to_string())
            .and_then(|popup| crate::menu::check(&popup.items).map(|()| popup));
        let popup = match popup {
            Ok(popup) => popup,
            Err(error) => {
                self.send_bridge_response(request.browser_id, request.request_id, Err(error));
                return;
            }
        };
        let zoom = self.page_zoom as f32;
        let anchor = (popup.x * zoom, popup.y * zoom + self.titlebar_height());
        let entries = app_items(&popup.items, &self.config.title);
        let origin = MenuOrigin::Popup {
            browser_id: request.browser_id.to_string(),
            request_id: request.request_id.to_string(),
        };
        if !self.show_menu(anchor, entries, origin, popup.items) {
            self.send_bridge_response(
                request.browser_id,
                request.request_id,
                Ok(serde_json::Value::Null),
            );
        }
    }
}
