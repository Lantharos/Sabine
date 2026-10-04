use winit::{
    cursor::CursorIcon,
    dpi::PhysicalPosition,
    event::{ButtonSource, ElementState, MouseButton, PointerKind, PointerSource},
    event_loop::ActiveEventLoop,
};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::ui::chrome::{activate_control, resize_direction_at};

impl OsrNativeHost {
    pub(super) fn pointer_moved(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        position: PhysicalPosition<f64>,
        source: PointerSource,
        primary: bool,
    ) {
        let (x, y) = self.logical_point(position);
        match source {
            PointerSource::Touch { finger_id, force } => {
                self.touch_moved(finger_id, force, x, y);
            }
            PointerSource::TabletTool { kind, data } => self.pen_moved(kind, data, x, y),
            PointerSource::Mouse | PointerSource::Unknown
                if primary && !self.move_page_drag(event_loop, x, y) =>
            {
                self.mouse_moved(x, y)
            }
            _ => {}
        }
    }

    pub(super) fn pointer_left(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        position: Option<PhysicalPosition<f64>>,
        kind: PointerKind,
        primary: bool,
    ) {
        let (x, y) = position.map_or((self.cursor_x, self.cursor_y), |position| {
            self.logical_point(position)
        });
        match kind {
            PointerKind::Touch(finger_id) => self.touch_left(finger_id, x, y),
            PointerKind::TabletTool(kind) => self.pen_left(kind, x, y),
            PointerKind::Mouse | PointerKind::Unknown if primary => {
                self.carry_page_drag_out(event_loop);
                self.mouse_left(x, y);
            }
            _ => {}
        }
    }

    pub(super) fn pointer_button(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        state: ElementState,
        position: PhysicalPosition<f64>,
        button: ButtonSource,
        primary: bool,
    ) {
        let (x, y) = self.logical_point(position);
        let pressed = state == ElementState::Pressed;
        match button {
            ButtonSource::Touch { finger_id, force } => {
                self.touch_button(finger_id, force, pressed, x, y);
            }
            ButtonSource::TabletTool { kind, button, data } => {
                self.pen_button(kind, button, data, pressed, x, y);
            }
            ButtonSource::Mouse(_) | ButtonSource::Unknown(_) if primary => {
                self.cursor_x = x;
                self.cursor_y = y;
                let button = button.mouse_button();
                if pressed {
                    self.mouse_pressed(button);
                } else {
                    self.mouse_released(event_loop, button);
                }
            }
            _ => {}
        }
    }

    pub(super) fn mouse_moved(&mut self, x: f32, y: f32) {
        self.cursor_x = x;
        self.cursor_y = y;
        let titlebar_changed = self.update_titlebar_hover();
        if self.config.resizable
            && let Some(direction) =
                resize_direction_at(x, y, self.logical_width(), self.logical_height())
        {
            self.set_native_cursor(CursorIcon::from(direction));
        } else if self.hovered_control.is_some() {
            self.set_native_cursor(CursorIcon::Pointer);
            self.forward_mouse_move(false);
        } else if self.content_position(x, y).is_some() {
            self.clear_native_cursor();
            self.forward_mouse_move(false);
        } else {
            self.set_native_cursor(CursorIcon::Default);
        }
        if titlebar_changed && let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(super) fn mouse_left(&mut self, x: f32, y: f32) {
        self.cursor_x = x;
        self.cursor_y = y;
        let titlebar_changed = self.hovered_control.take().is_some();
        self.forward_mouse_move(true);
        self.set_native_cursor(CursorIcon::Default);
        if titlebar_changed && let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn mouse_pressed(&mut self, button: Option<MouseButton>) {
        if matches!(button, Some(MouseButton::Back | MouseButton::Forward)) {
            return;
        }
        let Some(window) = self.window.clone() else {
            return;
        };
        let left = button == Some(MouseButton::Left);
        let (x, y, width) = (self.cursor_x, self.cursor_y, self.logical_width());
        if left
            && self.config.resizable
            && let Some(direction) = resize_direction_at(x, y, width, self.logical_height())
        {
            if let Err(error) = window.drag_resize_window(direction) {
                sabine_runtime::report_error(
                    "window",
                    format!("could not begin resizing: {error}"),
                );
            }
            return;
        }
        if left && let Some(control) = self.control_at(width, x, y) {
            self.pressed_control = Some(control);
            window.request_redraw();
            return;
        }
        if left && self.is_drag_region(width, x, y) {
            if let Err(error) = window.drag_window() {
                sabine_runtime::report_error("window", format!("could not begin moving: {error}"));
            }
            return;
        }
        #[cfg(target_os = "linux")]
        if button == Some(MouseButton::Middle) {
            self.note_middle_click();
        }
        self.active_click_count = self.next_click_count(button);
        self.set_mouse_button(button, true);
        self.forward_mouse_click(button, false, self.active_click_count);
    }

    fn mouse_released(&mut self, event_loop: &dyn ActiveEventLoop, button: Option<MouseButton>) {
        if let Some(pressed) = self.pressed_control.take() {
            let Some(window) = self.window.clone() else {
                return;
            };
            if self.control_at(self.logical_width(), self.cursor_x, self.cursor_y) == Some(pressed)
            {
                activate_control(self, event_loop, &window, pressed);
            }
            window.request_redraw();
            return;
        }
        if matches!(button, Some(MouseButton::Back | MouseButton::Forward)) {
            self.forward_navigation_button(button);
            return;
        }
        if !self.mouse_button_pressed(button) {
            return;
        }
        self.set_mouse_button(button, false);
        if button == Some(MouseButton::Left) && self.drop_page_drag(self.cursor_x, self.cursor_y) {
            return;
        }
        self.forward_mouse_click(button, true, self.active_click_count);
    }
}
