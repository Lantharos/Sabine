use std::collections::VecDeque;

use winit::{cursor::CursorIcon, event_loop::ActiveEventLoop};

use crate::osr::protocol::{OsrMessage, POPUP_OVERLAY_ID};

use super::native::OsrNativeHost;
pub(super) use super::types::HostActivity;
use super::types::{HostControl, OsrHostEvent, overlay_texture_id};
use super::visibility::{activation_token_value, bool_control_value};

const HOST_EVENT_DISPATCH_BUDGET: usize = 16;

struct PaintOutcome {
    updated: bool,
    resize_frame_ready: bool,
    initial_present: bool,
}

impl OsrNativeHost {
    pub(super) fn process_osr_events(&mut self, event_loop: &dyn ActiveEventLoop) {
        let mut needs_redraw = false;
        let mut needs_initial_present = false;
        let mut resize_frame_ready = false;
        let mut message_budget_used = false;
        let mut events = VecDeque::new();
        if let Some((generation, messages)) = self.pending_messages.take()
            && generation == self.connection_generation
        {
            let (queued, remaining) = messages.drain_budgeted();
            if remaining {
                self.pending_messages = Some((generation, std::sync::Arc::clone(&messages)));
                self.proxy.wake_up();
            }
            events.extend(
                queued
                    .into_iter()
                    .map(|message| OsrHostEvent::Message(generation, message)),
            );
            message_budget_used = true;
        }
        let mut received = 0;
        while received < HOST_EVENT_DISPATCH_BUDGET {
            let Ok(event) = self.receiver.try_recv() else {
                break;
            };
            events.push_back(event);
            received += 1;
        }
        if received == HOST_EVENT_DISPATCH_BUDGET {
            self.proxy.wake_up();
        }
        while let Some(event) = events.pop_front() {
            if event
                .connection_generation()
                .is_some_and(|generation| generation != self.connection_generation)
            {
                continue;
            }
            match event {
                OsrHostEvent::MessagesReady(generation, messages) => {
                    if message_budget_used {
                        if self.pending_messages.is_none() {
                            self.pending_messages = Some((generation, messages));
                        }
                        self.proxy.wake_up();
                        continue;
                    }
                    let (queued, remaining) = messages.drain_budgeted();
                    if remaining {
                        self.pending_messages =
                            Some((generation, std::sync::Arc::clone(&messages)));
                        self.proxy.wake_up();
                    }
                    for message in queued.into_iter().rev() {
                        events.push_front(OsrHostEvent::Message(generation, message));
                    }
                    message_budget_used = true;
                }
                OsrHostEvent::Connected(_, stream, writer) => {
                    if self.closing_deadline.is_some() {
                        let _ = stream.shutdown(std::net::Shutdown::Both);
                        self.awaiting_connection = false;
                        continue;
                    }
                    self.socket = Some(stream);
                    self.relay.connect(Some(std::sync::Arc::clone(&writer)));
                    self.control_writer = Some(writer);
                    self.awaiting_connection = false;
                    self.connection_deadline = None;
                    let mut output = std::io::stdout();
                    use std::io::Write;
                    let _ = writeln!(output, "SABINE_OSR_READY");
                    let _ = output.flush();
                    self.handoff_deadline = None;
                    self.send_resize();
                    self.send_current_lifecycle();
                    self.send_window_state();
                    self.send_control(if self.focused {
                        "focus\t1\n"
                    } else {
                        "focus\t0\n"
                    });
                }
                OsrHostEvent::Message(_, OsrMessage::PaintBatch(batch)) => {
                    if self.accepts_paint() {
                        let paint = self.apply_paint(|host| host.update_paint_batch(batch));
                        needs_redraw |= paint.updated;
                        resize_frame_ready |= paint.resize_frame_ready;
                        needs_initial_present |= paint.initial_present;
                    }
                }
                #[cfg(any(windows, target_os = "macos"))]
                OsrHostEvent::Message(_, OsrMessage::AccelFrame(frame)) => {
                    if self.accepts_paint() {
                        let paint = self.apply_paint(|host| host.update_accel_frame(frame));
                        needs_redraw |= paint.updated;
                        resize_frame_ready |= paint.resize_frame_ready;
                        needs_initial_present |= paint.initial_present;
                    } else {
                        self.send_control(&format!("accel_release\t{}\n", frame.slot_token));
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::PopupHidden) => {
                    self.clear_overlay(POPUP_OVERLAY_ID);
                    needs_redraw = true;
                }
                OsrHostEvent::Message(_, OsrMessage::GuestHidden(id)) => {
                    if !id.is_empty() {
                        self.clear_overlay(&id);
                        needs_redraw = true;
                    }
                }
                OsrHostEvent::Message(
                    _,
                    OsrMessage::GuestCaptureRequested {
                        browser_id,
                        request_id,
                        guest_id,
                    },
                ) => self.capture_guest(&browser_id, &request_id, &guest_id),
                OsrHostEvent::Message(
                    _,
                    OsrMessage::DraggableRegionsChanged { drag, exclusion },
                ) => {
                    self.page_drag_regions = drag;
                    self.page_drag_exclusion_regions = exclusion;
                }
                OsrHostEvent::Message(_, OsrMessage::Cursor(cursor)) => {
                    self.set_content_cursor(cursor_for_cef(&cursor));
                }
                OsrHostEvent::Message(_, OsrMessage::CloseRequested) => {
                    if self.config.hide_on_close {
                        self.hide_window("close");
                    } else {
                        self.begin_close(event_loop);
                        return;
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::StartDragRequested) => {
                    let button = Some(winit::event::MouseButton::Left);
                    if self.mouse_button_pressed(button) {
                        self.set_mouse_button(button, false);
                        self.forward_mouse_click(button, true, self.active_click_count);
                    }
                    if let Some(window) = &self.window
                        && let Err(error) = window.drag_window()
                    {
                        eprintln!("failed to begin native window drag: {error}");
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::FileDragRequested(request)) => {
                    self.start_file_drag(event_loop, request);
                }
                OsrHostEvent::Message(_, OsrMessage::MinimizeRequested) => {
                    if self.config.lifecycle.suspend_on_minimize {
                        self.suspend("minimize");
                        if self.config.lifecycle.hibernate_after.is_some() {
                            self.begin_hibernate("minimize");
                        }
                    }
                    if let Some(window) = &self.window {
                        window.set_minimized(true);
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::MaximizeRequested) => {
                    if let Some(window) = &self.window {
                        window.set_maximized(true);
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::RestoreRequested) => {
                    if let Some(window) = &self.window {
                        window.set_fullscreen(None);
                        window.set_maximized(false);
                        window.set_minimized(false);
                    }
                    self.resume("restore");
                }
                OsrHostEvent::Message(_, OsrMessage::ToggleMaximizeRequested) => {
                    if let Some(window) = &self.window {
                        window.set_maximized(!window.is_maximized());
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::FullscreenRequested(enabled)) => {
                    if let Some(window) = &self.window {
                        window.set_fullscreen(
                            enabled.then_some(winit::monitor::Fullscreen::Borderless(None)),
                        );
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::ShowRequested) => {
                    self.ensure_window(event_loop);
                    self.show_window("show");
                }
                OsrHostEvent::Message(_, OsrMessage::HideRequested) => self.hide_window("hide"),
                OsrHostEvent::Message(_, OsrMessage::FocusRequested(token)) => {
                    self.activate_window(event_loop, token);
                }
                OsrHostEvent::Message(_, OsrMessage::BridgeRequest(frame)) => {
                    let answered =
                        frame.body.is_none() && self.answer_window_bridge_request(&frame.line);
                    if !frame.line.is_empty() && !answered {
                        let mut output = std::io::stdout();
                        use std::io::Write;
                        let _ = output.write_all(&frame.to_bytes());
                        let _ = output.flush();
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::MainLoadStarted) => {
                    super::trace_host(&self.config, "browser.load_started");
                    #[cfg(target_os = "linux")]
                    self.clear_media();
                    self.main_load_ready = false;
                    self.main_frame_presented = false;
                    if self.config.visible && self.loading.is_none() {
                        self.loading = Some(super::types::NativeLoading::new(
                            super::types::LoadingKind::Opening,
                        ));
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::FatalError(message)) => {
                    self.fail(message);
                    self.force_close(event_loop);
                    return;
                }
                OsrHostEvent::Message(_, OsrMessage::MainLoadReady) => {
                    super::trace_host(&self.config, "browser.load_ready");
                    self.main_load_ready = true;
                    if self.main_surface.is_some() {
                        self.loading = None;
                        needs_redraw = true;
                        needs_initial_present |= !self.presented;
                    }
                }
                OsrHostEvent::Message(_, OsrMessage::ImeStateChanged(mode)) => {
                    self.update_ime_state(mode);
                }
                OsrHostEvent::Message(
                    _,
                    OsrMessage::ImeCursorAreaChanged {
                        x,
                        y,
                        width,
                        height,
                    },
                ) => self.update_ime_cursor_area(x, y, width, height),
                OsrHostEvent::Message(_, OsrMessage::TooltipChanged(text)) => {
                    needs_redraw |= self.update_tooltip(text);
                }
                OsrHostEvent::Message(
                    _,
                    OsrMessage::ImeSurroundingChanged {
                        text,
                        cursor_utf16,
                        anchor_utf16,
                        base_utf16,
                    },
                ) => self.update_ime_surrounding(text, cursor_utf16, anchor_utf16, base_utf16),
                OsrHostEvent::HostControl(HostControl::Show) => {
                    self.ensure_window(event_loop);
                    self.show_window("show");
                }
                OsrHostEvent::HostControl(HostControl::Hide) => self.hide_window("hide"),
                OsrHostEvent::HostControl(HostControl::Focus(token)) => {
                    self.activate_window(event_loop, token);
                }
                OsrHostEvent::HostControl(HostControl::Visible(true)) => {
                    self.ensure_window(event_loop);
                    self.show_window("visible")
                }
                OsrHostEvent::HostControl(HostControl::Visible(false)) => {
                    self.hide_window("hidden")
                }
                OsrHostEvent::HostControl(HostControl::Regions(regions)) => {
                    self.set_regions(regions)
                }
                OsrHostEvent::HostControl(HostControl::Quit) => {
                    self.begin_close(event_loop);
                    return;
                }
                OsrHostEvent::HostControl(HostControl::ActivityBegin(activity)) => {
                    self.begin_activity(activity)
                }
                OsrHostEvent::HostControl(HostControl::ActivityEnd(activity)) => {
                    self.end_activity(activity)
                }
                OsrHostEvent::IncompatibleHost(_) => {
                    self.fail(
                        "The shared Sabine host is out of date for this app. Update Sabine, then open the app again."
                            .to_string(),
                    );
                    self.force_close(event_loop);
                    return;
                }
                OsrHostEvent::Disconnected(_) => {
                    #[cfg(target_os = "linux")]
                    self.clear_media();
                    self.drop_connection();
                    self.awaiting_connection = false;
                    self.connection_deadline = None;
                    if self.closing_deadline.is_some() {
                        continue;
                    }
                    if matches!(
                        self.lifecycle_state,
                        super::types::LifecycleState::Hibernating
                            | super::types::LifecycleState::Hibernated
                    ) {
                        self.lifecycle_state = super::types::LifecycleState::Hibernated;
                        continue;
                    }
                    self.begin_recovery();
                }
            }
        }
        if needs_initial_present {
            if self.render() {
                self.present_rendered_surface("first_paint");
            }
            return;
        }
        if self.config.visible
            && needs_redraw
            && let Some(window) = &self.window
        {
            if resize_frame_ready && self.presented {
                self.render();
            } else {
                window.request_redraw();
            }
        }
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

    pub(super) fn capture_guest(&self, browser_id: &str, request_id: &str, guest_id: &str) {
        let result = self
            .renderer
            .as_ref()
            .filter(|_| self.overlays.contains_key(guest_id))
            .and_then(|renderer| renderer.read_bgra_image(&overlay_texture_id(guest_id)))
            .ok_or_else(|| "guest has no frame to capture".to_string())
            .and_then(|frame| {
                super::paint::guest_preview::guest_preview_data_url(
                    &frame.bytes,
                    frame.width,
                    frame.height,
                )
            });
        self.send_bridge_response(
            browser_id,
            request_id,
            result.map(|data_url| serde_json::json!({ "dataUrl": data_url })),
        );
    }

    pub(super) fn send_bridge_response(
        &self,
        browser_id: &str,
        request_id: &str,
        result: Result<serde_json::Value, String>,
    ) {
        self.send_control(&bridge_response_line(browser_id, request_id, result));
    }
}

pub(super) fn bridge_response_line(
    browser_id: &str,
    request_id: &str,
    result: Result<serde_json::Value, String>,
) -> String {
    let (status, payload) = match result {
        Ok(payload) => ("ok", payload),
        Err(message) => ("error", serde_json::json!({ "message": message })),
    };
    format!("SABINE_BRIDGE_RESPONSE\t{browser_id}\t{request_id}\t{status}\t{payload}\n")
}

pub(super) fn host_control_from_parts(command: &str, value: &str) -> Option<HostControl> {
    match command {
        "visible" => bool_control_value(value).map(HostControl::Visible),
        "show" => Some(HostControl::Show),
        "hide" => Some(HostControl::Hide),
        "quit" => Some(HostControl::Quit),
        "focus" => Some(HostControl::Focus(activation_token_value(Some(
            value.to_string(),
        )))),
        "activity.begin" => activity_control_value(value).map(HostControl::ActivityBegin),
        "activity.end" => activity_control_value(value).map(HostControl::ActivityEnd),
        "regions" => serde_json::from_str(value).ok().map(|regions| {
            HostControl::Regions(crate::osr::protocol::regions_from_json(Some(&regions)))
        }),
        _ => None,
    }
}

fn activity_control_value(value: &str) -> Option<HostActivity> {
    let value = serde_json::from_str::<serde_json::Value>(value).ok()?;
    Some(HostActivity {
        id: value.get("id")?.as_str()?.to_string(),
        prevents_hibernation: value
            .get("preventsHibernation")
            .or_else(|| value.get("prevents_hibernation"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
    })
}

fn cursor_for_cef(cursor: &str) -> CursorIcon {
    match cursor {
        "pointer" | "hand" => CursorIcon::Pointer,
        "text" | "vertical-text" => CursorIcon::Text,
        "crosshair" => CursorIcon::Crosshair,
        "move" => CursorIcon::Move,
        "wait" => CursorIcon::Wait,
        "help" => CursorIcon::Help,
        "not-allowed" => CursorIcon::NotAllowed,
        "col-resize" | "ew-resize" => CursorIcon::EwResize,
        "row-resize" | "ns-resize" => CursorIcon::NsResize,
        "ne-resize" => CursorIcon::NeResize,
        "nw-resize" => CursorIcon::NwResize,
        "se-resize" => CursorIcon::SeResize,
        "sw-resize" => CursorIcon::SwResize,
        _ => CursorIcon::Default,
    }
}
