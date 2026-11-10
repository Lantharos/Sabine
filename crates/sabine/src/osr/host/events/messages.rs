use winit::event_loop::ActiveEventLoop;

use crate::osr::protocol::{OsrMessage, POPUP_OVERLAY_ID};

use super::{Flow, Repaint};
use crate::osr::host::native::OsrNativeHost;

struct PaintOutcome {
    updated: bool,
    resize_frame_ready: bool,
    initial_present: bool,
}

impl Repaint {
    fn absorb(&mut self, paint: PaintOutcome) {
        self.redraw |= paint.updated;
        self.resize_frame_ready |= paint.resize_frame_ready;
        self.initial_present |= paint.initial_present;
    }
}

impl OsrNativeHost {
    pub(super) fn dispatch_message(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        message: OsrMessage,
        repaint: &mut Repaint,
    ) -> Flow {
        match message {
            OsrMessage::PaintBatch(batch) => {
                if self.accepts_paint() {
                    self.note_page_frame();
                    repaint.absorb(self.apply_paint(|host| host.update_paint_batch(batch)));
                }
            }
            OsrMessage::AccelFrame(frame) => {
                if self.accepts_paint() {
                    self.note_page_frame();
                    repaint.absorb(self.apply_paint(|host| host.update_accel_frame(frame)));
                } else {
                    self.send_control(format!("accel_release\t{}\n", frame.slot_token));
                }
            }
            OsrMessage::AccelUnavailable => {
                self.paint_in_software("the browser could not share its frames");
                #[cfg(target_os = "linux")]
                self.draw_without_direct_presentation(event_loop);
            }
            OsrMessage::PopupHidden => {
                self.clear_overlay(POPUP_OVERLAY_ID);
                repaint.redraw = true;
            }
            OsrMessage::GuestHidden(id) => {
                if !id.is_empty() {
                    self.clear_overlay(&id);
                    repaint.redraw = true;
                }
            }
            OsrMessage::GuestOrder(order) => {
                self.overlays.stack(order);
                repaint.redraw = true;
            }
            OsrMessage::GuestCaptureRequested {
                browser_id,
                request_id,
                guest_id,
            } => self.capture_guest(&browser_id, &request_id, &guest_id),
            OsrMessage::DraggableRegionsChanged { drag, exclusion } => {
                self.page_drag_regions = drag;
                self.page_drag_exclusion_regions = exclusion;
            }
            OsrMessage::Cursor(cursor) => self.set_page_cursor(event_loop, cursor),
            OsrMessage::CloseRequested => return self.close_window(event_loop),
            OsrMessage::StartDragRequested => self.start_window_drag(),
            OsrMessage::DragStarted {
                content,
                operations,
                image,
            } => self.begin_page_drag(content, operations, image),
            OsrMessage::ContextMenu { x, y, items } => self.open_context_menu(x, y, items),
            OsrMessage::FileDialog(request) => self.show_file_dialog(request),
            #[cfg(target_os = "linux")]
            OsrMessage::PrintDialog(request) => self.show_page_print_dialog(request),
            #[cfg(target_os = "linux")]
            OsrMessage::PrintJob(job) => self.print_page(job),
            OsrMessage::PrintDocument(outcome) => self.finish_print_document(outcome),
            OsrMessage::MediaPlaying(playing) => self.set_page_media_playing(playing),
            OsrMessage::ContextMenuDismissed => self.dismiss_context_menu(),
            OsrMessage::DragOperation(operation) => {
                self.update_drag_operation(event_loop, operation);
            }
            OsrMessage::MinimizeRequested => self.minimize_window(),
            OsrMessage::MaximizeRequested => {
                if let Some(window) = &self.window {
                    window.set_maximized(true);
                }
            }
            OsrMessage::RestoreRequested => {
                if let Some(window) = &self.window {
                    window.set_fullscreen(None);
                    window.set_maximized(false);
                    window.set_minimized(false);
                }
                self.resume("restore");
            }
            OsrMessage::ToggleMaximizeRequested => {
                if let Some(window) = &self.window {
                    window.set_maximized(!window.is_maximized());
                }
            }
            OsrMessage::FullscreenRequested(enabled) => {
                if let Some(window) = &self.window {
                    window.set_fullscreen(
                        enabled.then_some(winit::monitor::Fullscreen::Borderless(None)),
                    );
                }
            }
            OsrMessage::ShowRequested => {
                self.ensure_window(event_loop);
                self.show_window("show");
            }
            OsrMessage::HideRequested => self.hide_window("hide"),
            OsrMessage::FocusRequested(token) => self.activate_window(event_loop, token),
            OsrMessage::BridgeRequest(frame) => self.relay_bridge_request(frame),
            OsrMessage::MainLoadStarted { page_lost } => {
                crate::osr::host::trace_host(&self.config, "browser.load_started");
                if page_lost {
                    self.report_page_lost();
                } else {
                    self.report_page_started();
                }
                self.dismiss_context_menu();
                self.clear_media();
                self.main_load_ready = false;
                self.main_frame_presented = false;
                self.reveal_held_since = None;
                if self.config.visible && self.loading.is_none() {
                    self.start_loading();
                }
            }
            OsrMessage::FatalError(message) => {
                self.fail(message);
                self.force_close(event_loop);
                return Flow::Closed;
            }
            OsrMessage::MainLoadReady => {
                crate::osr::host::trace_host(&self.config, "browser.load_ready");
                self.main_load_ready = true;
                self.note_main_load_ready();
                if self.main_surface_ready() {
                    self.finish_loading();
                    repaint.redraw = true;
                    repaint.initial_present |= !self.presented;
                }
            }
            OsrMessage::ImeStateChanged(mode) => self.update_ime_state(mode),
            OsrMessage::ImeCursorAreaChanged {
                x,
                y,
                width,
                height,
            } => self.update_ime_cursor_area(x, y, width, height),
            OsrMessage::TooltipChanged(text) => repaint.redraw |= self.update_tooltip(text),
            OsrMessage::ImeSurroundingChanged {
                text,
                cursor_utf16,
                anchor_utf16,
                base_utf16,
            } => self.update_ime_surrounding(text, cursor_utf16, anchor_utf16, base_utf16),
        }
        Flow::Continue
    }

    fn apply_paint(&mut self, update: impl FnOnce(&mut Self) -> bool) -> PaintOutcome {
        let was_presented = self.presented;
        let was_resize_pending = self.pending_resize_paint.is_some();
        let updated = update(self);
        PaintOutcome {
            updated,
            resize_frame_ready: was_resize_pending && self.pending_resize_paint.is_none(),
            initial_present: !was_presented && self.main_surface_ready(),
        }
    }

    fn start_window_drag(&mut self) {
        let button = Some(winit::event::MouseButton::Left);
        if self.mouse_button_pressed(button) {
            self.set_mouse_button(button, false);
            self.forward_mouse_click(button, true, self.active_click_count);
        }
        if let Some(window) = &self.window
            && let Err(error) = window.drag_window()
        {
            sabine_runtime::report_error("window", format!("could not begin moving: {error}"));
        }
    }
}
