use std::future::Future;
use std::path::PathBuf;

use sabine_platform::shell::Requester;
use serde::Deserialize;

use super::BridgeRequest;
use crate::osr::host::events::bridge_response_line;
use crate::osr::host::native::OsrNativeHost;

#[derive(Deserialize)]
struct OpenLink {
    url: String,
}

#[derive(Deserialize)]
struct ShowFolder {
    path: PathBuf,
}

impl OsrNativeHost {
    pub(super) fn answer_open_link(&mut self, request: &BridgeRequest) {
        match serde_json::from_str::<OpenLink>(request.payload) {
            Ok(OpenLink { url }) => self.answer_later(request, move |requester| async move {
                sabine_platform::shell::open_link(&url, requester).await
            }),
            Err(error) => self.send_bridge_response(
                request.browser_id,
                request.request_id,
                Err(error.to_string()),
            ),
        }
    }

    pub(super) fn answer_show_folder(&mut self, request: &BridgeRequest) {
        match serde_json::from_str::<ShowFolder>(request.payload) {
            Ok(ShowFolder { path }) => self.answer_later(request, move |requester| async move {
                sabine_platform::shell::show_folder(&path, requester).await
            }),
            Err(error) => self.send_bridge_response(
                request.browser_id,
                request.request_id,
                Err(error.to_string()),
            ),
        }
    }

    /// Answers the page once the desktop has taken the request, without
    /// holding up the window meanwhile.
    fn answer_later<F, Fut>(&self, request: &BridgeRequest, open: F)
    where
        F: FnOnce(Requester) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send,
    {
        let Some(writer) = self.control_writer.clone() else {
            return;
        };
        let browser_id = request.browser_id.to_string();
        let request_id = request.request_id.to_string();
        self.with_requester(move |requester| {
            sabine_platform::async_runtime().spawn(async move {
                let result = open(requester).await.map(|()| serde_json::Value::Null);
                let _ = writer.send(bridge_response_line(&browser_id, &request_id, result));
            });
        });
    }

    /// On Wayland the desktop keeps its dialogs over the window and lets
    /// what opens take focus, with a token made while the person uses it.
    #[cfg(target_os = "linux")]
    fn with_requester(&self, then: impl FnOnce(Requester) + Send + 'static) {
        let mut requester = Requester {
            app_id: self.config.app_id.clone(),
            parent_window: self.window.as_ref().and_then(|window| {
                sabine_platform::shell::ParentWindow::export(window.as_ref()).ok()
            }),
            activation_token: None,
        };
        match &self.clipboard {
            Some(clipboard) => clipboard.activation_token(Box::new(move |token| {
                requester.activation_token = token;
                then(requester);
            })),
            None => then(requester),
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn with_requester(&self, then: impl FnOnce(Requester)) {
        then(Requester {
            app_id: self.config.app_id.clone(),
        });
    }
}
