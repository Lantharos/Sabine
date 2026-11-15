use std::io::Write;

use sabine_bridge::menu::{
    ACTIVATE_COMMAND, APP_MENU_COMMAND, POPUP_COMMAND, SET_APP_MENU_COMMAND,
    SET_CONTEXT_MENU_COMMAND,
};
use serde::Deserialize;

use crate::bridge::APP_MENU_LINE;
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::page::BridgeRequest;
use crate::{MenuItem, MenuKind};

#[derive(Deserialize)]
struct Items {
    items: Vec<MenuItem>,
}

#[derive(Deserialize)]
struct Activation {
    id: String,
}

impl OsrNativeHost {
    /// Answers a page's `menu` calls.
    pub(in crate::osr::host) fn answer_menu(&mut self, request: &BridgeRequest) {
        let result = match request.command {
            POPUP_COMMAND => return self.open_popup_menu(request),
            SET_CONTEXT_MENU_COMMAND => {
                items(request).map(|items| self.set_context_menu_items(items))
            }
            SET_APP_MENU_COMMAND => items(request).map(|items| {
                let mut output = std::io::stdout().lock();
                let _ = writeln!(output, "{APP_MENU_LINE}\t{}", serde_json::json!(items));
                let _ = output.flush();
                self.set_app_menu(items);
            }),
            APP_MENU_COMMAND => {
                return self.send_bridge_response(
                    request.browser_id,
                    request.request_id,
                    Ok(serde_json::json!(crate::menu::shown(
                        &self.menus.bar,
                        &self.config.title
                    ))),
                );
            }
            ACTIVATE_COMMAND => serde_json::from_str::<Activation>(request.payload)
                .map_err(|error| error.to_string())
                .and_then(|activation| {
                    let bar = self.menus.bar.clone();
                    crate::menu::find(&bar, &activation.id)
                        .ok_or_else(|| format!("the app menu has no item {}", activation.id))?;
                    self.choose_menu_item(&bar, &activation.id, MenuKind::App);
                    Ok(())
                }),
            command => Err(format!("{command} is not a menu command")),
        };
        self.send_bridge_response(
            request.browser_id,
            request.request_id,
            result.map(|()| serde_json::Value::Null),
        );
    }
}

fn items(request: &BridgeRequest) -> Result<Vec<MenuItem>, String> {
    let Items { items } =
        serde_json::from_str(request.payload).map_err(|error| error.to_string())?;
    crate::menu::check(&items)?;
    Ok(items)
}
