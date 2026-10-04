use std::io::Write;

use winit::event_loop::ActiveEventLoop;

use crate::osr::host::native::{OsrNativeHost, present_window};
use crate::osr::host::types::{LifecycleState, WindowState};
use crate::window::VISIBILITY_LINE;

impl OsrNativeHost {
    pub(in crate::osr::host) fn show_window(&mut self, reason: &str) {
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

    pub(in crate::osr::host) fn hide_window(&mut self, reason: &str) {
        self.cancel_context_menu();
        self.config.visible = false;
        self.focused = false;
        self.overlays.clear();
        self.send_control("focus\t0\n");
        self.unmap_window();
        self.suspend(reason);
    }

    pub(in crate::osr::host) fn focus_window(&mut self, reason: &str) {
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

    pub(in crate::osr::host) fn activate_window(
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

impl OsrNativeHost {
    fn window_state(&self) -> WindowState {
        WindowState {
            shown: self.config.visible,
            occluded: self.occluded,
            suspended: self.lifecycle_state != LifecycleState::Active,
        }
    }

    /// Tells the page and the app when the window's visibility or lifecycle
    /// changed since they last heard.
    pub(in crate::osr::host) fn sync_window_state(&mut self) {
        let state = self.window_state();
        let Some(previous) = self.published_window_state.replace(state) else {
            self.send_window_state();
            self.report_visibility(state);
            return;
        };
        if previous == state {
            return;
        }
        self.send_window_state();
        if previous.visible() != state.visible() || previous.suspended != state.suspended {
            self.report_visibility(state);
        }
    }

    pub(in crate::osr::host) fn send_window_state(&self) {
        let Some(state) = self.published_window_state else {
            return;
        };
        self.send_control(format!(
            "window_state\t{}\t{}\t{}\n",
            u8::from(state.shown),
            u8::from(state.occluded),
            u8::from(state.suspended)
        ));
    }

    fn report_visibility(&self, state: WindowState) {
        let mut output = std::io::stdout().lock();
        let _ = writeln!(
            output,
            "{VISIBILITY_LINE}\t{}\t{}",
            u8::from(state.visible()),
            u8::from(state.suspended)
        );
        let _ = output.flush();
    }
}

pub(in crate::osr::host) fn bool_control_value(value: &str) -> Option<bool> {
    match value {
        "1" | "true" | "yes" | "show" | "visible" => Some(true),
        "0" | "false" | "no" | "hide" | "hidden" => Some(false),
        _ => None,
    }
}

pub(in crate::osr::host) fn activation_token_value(token: Option<String>) -> Option<String> {
    token
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
        .filter(|token| bool_control_value(token).is_none())
}
