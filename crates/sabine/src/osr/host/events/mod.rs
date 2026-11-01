mod bridge;
mod host_control;
mod messages;

use std::collections::VecDeque;
use std::sync::Arc;

use winit::event_loop::ActiveEventLoop;

use crate::osr::control::ControlWriter;
use crate::osr::transport::IpcStream;

use super::native::OsrNativeHost;
use super::types::OsrHostEvent;

pub(super) use bridge::bridge_response_line;
pub(super) use host_control::host_control_from_parts;

const HOST_EVENT_DISPATCH_BUDGET: usize = 16;

/// Whether the window keeps handling events, or closed while handling one.
enum Flow {
    Continue,
    Closed,
}

/// What the events handled in one pass asked of the window's next frame.
#[derive(Default)]
struct Repaint {
    redraw: bool,
    resize_frame_ready: bool,
    initial_present: bool,
}

impl OsrNativeHost {
    pub(super) fn process_osr_events(&mut self, event_loop: &dyn ActiveEventLoop) {
        let mut repaint = Repaint::default();
        let mut message_budget_used = false;
        let mut events = VecDeque::new();
        if let Some((generation, messages)) = self.pending_messages.take()
            && generation == self.connection_generation
        {
            let (queued, remaining) = messages.drain_budgeted();
            if remaining {
                self.pending_messages = Some((generation, Arc::clone(&messages)));
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
            let flow = match event {
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
                        self.pending_messages = Some((generation, Arc::clone(&messages)));
                        self.proxy.wake_up();
                    }
                    for message in queued.into_iter().rev() {
                        events.push_front(OsrHostEvent::Message(generation, message));
                    }
                    message_budget_used = true;
                    Flow::Continue
                }
                OsrHostEvent::Connected(_, stream, writer) => {
                    self.browser_connected(stream, writer);
                    Flow::Continue
                }
                OsrHostEvent::Message(_, message) => {
                    self.dispatch_message(event_loop, message, &mut repaint)
                }
                OsrHostEvent::HostControl(control) => self.apply_host_control(event_loop, control),
                OsrHostEvent::FrozenPageMessaged => {
                    self.thaw("message");
                    Flow::Continue
                }
                OsrHostEvent::FileDialogClosed(_, id, paths) => {
                    self.finish_file_dialog(id, paths);
                    Flow::Continue
                }
                OsrHostEvent::BrowserExited(generation, status) => {
                    self.browser_exited(event_loop, generation, status);
                    Flow::Continue
                }
                OsrHostEvent::RendererStarted(generation, renderer) => {
                    self.renderer_started(event_loop, generation, *renderer);
                    Flow::Continue
                }
                #[cfg(target_os = "linux")]
                OsrHostEvent::SharedRuntimeOpened(generation, opened) => {
                    self.shared_runtime_opened(generation, opened);
                    Flow::Continue
                }
                #[cfg(target_os = "linux")]
                OsrHostEvent::DirectPresentation(generation, outcome) => {
                    self.direct_presentation_changed(event_loop, generation, outcome);
                    Flow::Continue
                }
                OsrHostEvent::IncompatibleHost(_) => {
                    if self.refuse_incompatible_browser() {
                        self.force_close(event_loop);
                        Flow::Closed
                    } else {
                        Flow::Continue
                    }
                }
                OsrHostEvent::Disconnected(_) => {
                    self.browser_disconnected();
                    Flow::Continue
                }
            };
            if matches!(flow, Flow::Closed) {
                return;
            }
        }
        if repaint.initial_present {
            self.present_first_frame();
            return;
        }
        if self.config.visible
            && repaint.redraw
            && let Some(window) = &self.window
        {
            if repaint.resize_frame_ready && self.presented {
                self.render();
            } else {
                window.request_redraw();
            }
        }
    }

    fn browser_connected(&mut self, stream: IpcStream, writer: Arc<ControlWriter>) {
        if self.closing_deadline.is_some() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            self.awaiting_connection = false;
            return;
        }
        super::trace_host(&self.config, "browser.connected");
        self.socket = Some(stream);
        self.relay.connect(Some(Arc::clone(&writer)));
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
        self.send_toplevel_state();
        self.send_control(if self.focused {
            "focus\t1\n"
        } else {
            "focus\t0\n"
        });
    }

    fn browser_disconnected(&mut self) {
        self.abandon_printing();
        self.page_media_playing = false;
        self.clear_media();
        self.drop_connection();
        self.awaiting_connection = false;
        self.connection_deadline = None;
        if self.closing_deadline.is_none() {
            self.begin_recovery();
        }
    }
}
