mod drag;
mod drop;
mod forward;
mod ime;
mod pointer;
mod shortcuts;
mod surface;
mod touch;

pub(super) use drag::DragState;
pub(super) use forward::WheelRemainder;
pub(super) use shortcuts::ShortcutInhibition;
pub(super) use touch::TouchState;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    keyboard::{Key, NamedKey},
    window::WindowId,
};

use crate::osr::host::native::OsrNativeHost;

impl ApplicationHandler for OsrNativeHost {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        #[cfg(target_os = "linux")]
        self.connect_clipboard(event_loop);
        #[cfg(not(target_os = "linux"))]
        self.connect_clipboard();
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
        #[cfg(target_os = "linux")]
        self.refresh_appearance();
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
            WindowEvent::SurfaceResized(size) => self.surface_resized(size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor_changed(scale_factor)
            }
            WindowEvent::Focused(focused) => self.focus_changed(focused),
            WindowEvent::Occluded(occluded) => self.set_occluded(occluded),
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
                let pressed = event.state.is_pressed();
                let escape = event.logical_key == Key::Named(NamedKey::Escape);
                if self.context_menu.is_some() {
                    if pressed {
                        self.menu_key_pressed(&event.logical_key);
                    }
                } else if !(escape && pressed && self.cancel_page_drag()) {
                    self.send_key_event(&event);
                }
            }
            WindowEvent::Ime(ime) => self.forward_ime(ime),
            #[cfg(target_os = "windows")]
            WindowEvent::ThemeChanged(theme) => {
                self.retint_effect(theme);
                self.refresh_appearance();
            }
            #[cfg(not(target_os = "windows"))]
            WindowEvent::ThemeChanged(_) => self.refresh_appearance(),
            WindowEvent::Moved(_) => {
                self.send_screen_origin();
                self.sync_active_frame_rate();
            }
            WindowEvent::RedrawRequested if self.config.visible && self.presented => {
                self.render();
            }
            WindowEvent::PointerMoved {
                position,
                source,
                primary,
                ..
            } => self.pointer_moved(event_loop, position, source, primary),
            WindowEvent::PointerLeft {
                position,
                kind,
                primary,
                ..
            } => self.pointer_left(event_loop, position, kind, primary),
            WindowEvent::PointerButton {
                state,
                position,
                button,
                primary,
                ..
            } => self.pointer_button(event_loop, state, position, button, primary),
            WindowEvent::MouseWheel { .. } if self.context_menu.is_some() => {}
            WindowEvent::MouseWheel { delta, .. } => self.forward_mouse_wheel(delta),
            WindowEvent::PinchGesture { delta, .. } => self.forward_pinch(delta),
            WindowEvent::DragEntered { id, position } if !self.clipboard_owns_drops() => {
                self.drag_entered(event_loop, id, position);
            }
            WindowEvent::DragPosition { id, position, .. } => self.drag_moved(id, position),
            WindowEvent::DataTransferReceived { id, value, .. } => {
                self.drag_received(id, value.as_ref());
            }
            WindowEvent::DragDropped { id, .. } => self.drag_dropped(id),
            WindowEvent::DragLeft { id } => self.drag_left(id),
            WindowEvent::OutgoingDragDropped { id, action } if self.drag.outgoing() == Some(id) => {
                self.end_page_drag(action);
            }
            WindowEvent::OutgoingDragCanceled { id } if self.drag.outgoing() == Some(id) => {
                self.end_page_drag(None);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.drive_deadlines(event_loop);
        self.sync_window_state();
    }
}
