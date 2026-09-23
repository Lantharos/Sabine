use winit::event_loop::ActiveEventLoop;

use super::native::{OsrNativeHost, present_window};

impl OsrNativeHost {
    pub(super) fn show_window(&mut self, reason: &str) {
        self.config.visible = true;
        if let Some(window) = &self.window {
            if self.presented {
                window.set_visible(true);
                window.set_minimized(false);
                window.request_redraw();
            } else {
                window.set_visible(false);
            }
        }
        self.resume(reason);
        self.send_resize();
    }

    pub(super) fn hide_window(&mut self, reason: &str) {
        self.config.visible = false;
        self.focused = false;
        self.overlays.clear();
        self.send_control("focus\t0\n");
        self.unmap_window();
        self.suspend(reason);
    }

    pub(super) fn focus_window(&mut self, reason: &str) {
        self.config.visible = true;
        self.focused = true;
        if let Some(window) = &self.window {
            if self.presented {
                present_window(window);
            } else {
                window.set_visible(false);
            }
        }
        self.send_control("focus\t1\n");
        self.resume(reason);
        self.send_resize();
    }

    pub(super) fn activate_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        token: Option<String>,
    ) {
        if let Some(token) = activation_token_value(token) {
            self.pending_activation_token = Some(winit::window::ActivationToken::from_raw(token));
            if self.window.is_some() {
                self.drop_presented_window();
            }
        }
        self.ensure_window(event_loop);
        self.focus_window("focus");
    }
}

pub(super) fn bool_control_value(value: &str) -> Option<bool> {
    match value {
        "1" | "true" | "yes" | "show" | "visible" => Some(true),
        "0" | "false" | "no" | "hide" | "hidden" => Some(false),
        _ => None,
    }
}

pub(super) fn activation_token_value(token: Option<String>) -> Option<String> {
    token
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
        .filter(|token| bool_control_value(token).is_none())
}
