mod drag;
mod forward;
mod ime;
mod shortcuts;
mod touch;

#[cfg(not(target_os = "macos"))]
pub(super) use shortcuts::ShortcutInhibition;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    window::WindowId,
};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::ui::chrome::{activate_control, resize_direction_at};
use winit::cursor::CursorIcon;

impl ApplicationHandler for OsrNativeHost {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[cfg(target_os = "linux")]
        self.connect_clipboard(event_loop);
        if !self.config.visible {
            self.launch_child();
            return;
        }
        #[cfg(not(windows))]
        self.launch_child_before_window(event_loop);
        self.create_window(event_loop);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.recover_gpu();
        #[cfg(windows)]
        self.forward_system_keys();
        #[cfg(target_os = "linux")]
        self.deliver_clipboard_drops();
        self.process_osr_events(event_loop);
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.recover_gpu();
        let Some(window) = self.window.clone() else {
            return;
        };
        if id != window.id() {
            return;
        }
        match event {
            WindowEvent::CloseRequested if self.config.hide_on_close => self.hide_window("close"),
            WindowEvent::CloseRequested => self.begin_close(event_loop),
            WindowEvent::Destroyed if self.config.visible || self.closing_deadline.is_some() => {
                self.begin_close(event_loop)
            }
            WindowEvent::Destroyed => self.drop_hidden_window(),
            WindowEvent::SurfaceResized(size) => {
                self.sync_active_frame_rate();
                if size.width == 0 || size.height == 0 {
                    return;
                }
                let scale = window.scale_factor();
                // Wayland emits a configure after interactive move even when the
                // size did not change. Reconfiguring wgpu / Invalidating CEF
                // flashes — especially noticeable on the handed-off second window.
                if size == self.surface_size && (scale - self.scale_factor).abs() < f64::EPSILON {
                    return;
                }
                self.surface_size = size;
                self.scale_factor = scale;
                self.effect_regions_dirty = true;
                #[cfg(target_os = "linux")]
                self.relayout_media();
                self.queue_resize_paint();
                if self.presented {
                    self.render();
                } else {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.sync_active_frame_rate();
                let size = window.surface_size();
                self.surface_size = size;
                self.scale_factor = scale_factor;
                self.effect_regions_dirty = true;
                #[cfg(target_os = "linux")]
                self.relayout_media();
                self.queue_resize_paint();
                if self.presented {
                    self.render();
                } else {
                    window.request_redraw();
                }
            }
            WindowEvent::Focused(focused) => {
                self.sync_active_frame_rate();
                let focused = focused && self.config.visible;
                self.focused = focused;
                self.send_control(if focused { "focus\t1\n" } else { "focus\t0\n" });
                if !focused && self.config.hide_on_blur && self.config.visible {
                    self.hide_window("blur");
                } else {
                    self.schedule_lifecycle_sync(if focused { "focus" } else { "blur" });
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                #[cfg(target_os = "linux")]
                self.media.set_occluded(occluded);
                self.schedule_lifecycle_sync(if occluded { "occluded" } else { "visible" });
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                #[cfg(target_os = "linux")]
                self.note_paste_key(&event);
                self.send_key_event(&event);
            }
            WindowEvent::Ime(ime) => self.forward_ime(ime),
            WindowEvent::Moved(_) => {
                self.send_screen_origin();
                self.sync_active_frame_rate();
            }
            WindowEvent::RedrawRequested if self.config.visible && self.presented => {
                self.render();
            }
            WindowEvent::RedrawRequested => {}
            WindowEvent::PointerMoved {
                position, source, ..
            } if !matches!(
                source,
                winit::event::PointerSource::Mouse | winit::event::PointerSource::Unknown
            ) =>
            {
                let scale = window.scale_factor() as f32;
                let x = position.x as f32 / scale.max(1.0);
                let y = position.y as f32 / scale.max(1.0);
                self.cursor_x = x;
                self.cursor_y = y;
                self.forward_pointer_source(&source, "moved", x, y);
            }
            WindowEvent::PointerMoved {
                position, primary, ..
            } if primary => {
                let scale = window.scale_factor() as f32;
                self.cursor_x = position.x as f32 / scale.max(1.0);
                self.cursor_y = position.y as f32 / scale.max(1.0);
                let titlebar_changed = self.update_titlebar_hover();
                if self.config.resizable
                    && let Some(direction) = resize_direction_at(
                        self.cursor_x,
                        self.cursor_y,
                        self.logical_width(),
                        self.logical_height(),
                    )
                {
                    self.set_native_cursor(CursorIcon::from(direction));
                } else if self.hovered_control.is_some() {
                    self.set_native_cursor(CursorIcon::Pointer);
                    self.forward_mouse_move(false);
                } else if self
                    .content_position(self.cursor_x, self.cursor_y)
                    .is_some()
                {
                    self.clear_native_cursor();
                    self.forward_mouse_move(false);
                } else {
                    self.set_native_cursor(CursorIcon::Default);
                }
                if titlebar_changed {
                    window.request_redraw();
                }
            }
            WindowEvent::PointerLeft { position, kind, .. }
                if matches!(
                    kind,
                    winit::event::PointerKind::Touch(_) | winit::event::PointerKind::TabletTool(_)
                ) =>
            {
                let (x, y) = position
                    .map(|position| {
                        let scale = window.scale_factor() as f32;
                        (
                            position.x as f32 / scale.max(1.0),
                            position.y as f32 / scale.max(1.0),
                        )
                    })
                    .unwrap_or((self.cursor_x, self.cursor_y));
                let source = match kind {
                    winit::event::PointerKind::Touch(finger_id) => {
                        winit::event::PointerSource::Touch {
                            finger_id,
                            force: None,
                        }
                    }
                    winit::event::PointerKind::TabletTool(kind) => {
                        winit::event::PointerSource::TabletTool {
                            kind,
                            data: Default::default(),
                        }
                    }
                    _ => unreachable!(),
                };
                self.forward_pointer_source(&source, "cancelled", x, y);
            }
            WindowEvent::PointerLeft {
                position, primary, ..
            } if primary => {
                if let Some(position) = position {
                    let scale = window.scale_factor() as f32;
                    self.cursor_x = position.x as f32 / scale.max(1.0);
                    self.cursor_y = position.y as f32 / scale.max(1.0);
                }
                let titlebar_changed = self.hovered_control.take().is_some();
                self.forward_mouse_move(true);
                self.set_native_cursor(CursorIcon::Default);
                if titlebar_changed {
                    window.request_redraw();
                }
            }
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } if !matches!(
                button,
                winit::event::ButtonSource::Mouse(_) | winit::event::ButtonSource::Unknown(_)
            ) =>
            {
                let scale = window.scale_factor() as f32;
                let x = position.x as f32 / scale.max(1.0);
                let y = position.y as f32 / scale.max(1.0);
                self.cursor_x = x;
                self.cursor_y = y;
                let phase = if state == ElementState::Pressed {
                    "pressed"
                } else {
                    "released"
                };
                self.forward_pointer_button(&button, phase, x, y);
            }
            WindowEvent::PointerButton {
                state,
                primary,
                position,
                button,
                ..
            } if primary => {
                let scale = window.scale_factor() as f32;
                self.cursor_x = position.x as f32 / scale.max(1.0);
                self.cursor_y = position.y as f32 / scale.max(1.0);
                let button = button.clone().mouse_button();
                match state {
                    ElementState::Pressed => {
                        if matches!(button, Some(MouseButton::Back | MouseButton::Forward)) {
                            return;
                        }
                        let left = matches!(button, Some(MouseButton::Left));
                        if left
                            && self.config.resizable
                            && let Some(direction) = resize_direction_at(
                                self.cursor_x,
                                self.cursor_y,
                                self.logical_width(),
                                self.logical_height(),
                            )
                        {
                            if let Err(error) = window.drag_resize_window(direction) {
                                eprintln!("failed to begin native window resize: {error}");
                            }
                            return;
                        }
                        let width = self.logical_width();
                        if left
                            && let Some(control) =
                                self.control_at(width, self.cursor_x, self.cursor_y)
                        {
                            self.pressed_control = Some(control);
                            window.request_redraw();
                            return;
                        }
                        if left && self.is_drag_region(width, self.cursor_x, self.cursor_y) {
                            if let Err(error) = window.drag_window() {
                                eprintln!("failed to begin native window drag: {error}");
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
                    ElementState::Released => {
                        if let Some(pressed) = self.pressed_control.take() {
                            let released =
                                self.control_at(self.logical_width(), self.cursor_x, self.cursor_y);
                            if released == Some(pressed) {
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
                        self.forward_mouse_click(button, true, self.active_click_count);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.forward_mouse_wheel(delta);
            }
            WindowEvent::DragEntered { id, position } if !self.clipboard_owns_drops() => {
                self.begin_incoming_file_drag(event_loop, id, position);
            }
            WindowEvent::DragPosition {
                id,
                position,
                proposed_action,
            } => {
                self.update_incoming_file_drag(id, position, proposed_action);
            }
            WindowEvent::DataTransferReceived { id, value, .. } => {
                self.receive_incoming_file_drag(id, value.as_ref());
            }
            WindowEvent::DragDropped {
                id,
                proposed_action,
            } => {
                self.drop_incoming_file_drag(id, proposed_action);
            }
            WindowEvent::DragLeft { id } => self.leave_incoming_file_drag(id),
            WindowEvent::OutgoingDragDropped { id, action }
                if self.active_file_drag == Some(id) =>
            {
                self.finish_file_drag(action);
            }
            WindowEvent::OutgoingDragCanceled { id } if self.active_file_drag == Some(id) => {
                self.finish_file_drag(None);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.drive_deadlines(event_loop);
        self.sync_window_state();
    }
}
