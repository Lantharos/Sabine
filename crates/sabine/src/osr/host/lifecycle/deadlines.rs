use std::time::{Duration, Instant};

use winit::event_loop::{ActiveEventLoop, ControlFlow};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::LifecycleState;

impl OsrNativeHost {
    pub(in crate::osr::host) fn drive_deadlines(&mut self, event_loop: &dyn ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        let mut handoff = false;
        let mut exited = Vec::new();
        self.children
            .retain_mut(|(generation, child)| match child.try_wait() {
                Ok(Some(status)) => {
                    if *generation != self.connection_generation {
                        return false;
                    }
                    if status.code() == Some(CEF_RESULT_CODE_NORMAL_EXIT_PROCESS_NOTIFIED) {
                        handoff = true;
                    } else {
                        exited.push(status);
                    }
                    false
                }
                Ok(None) | Err(_) => true,
            });
        if let Some(deadline) = self.closing_deadline {
            if Instant::now() >= deadline {
                self.force_close(event_loop);
                return;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            return;
        }
        if handoff {
            self.cef_handed_off = true;
            self.handoff_deadline =
                Some(Instant::now() + Duration::from_secs(HANDOFF_CONNECT_TIMEOUT_SECS));
            crate::osr::host::trace_host(&self.config, "cef.handed_off.waiting_for_primary");
        }
        if !exited.is_empty() && self.socket.is_none() {
            self.awaiting_connection = false;
            if matches!(
                self.lifecycle_state,
                LifecycleState::Hibernating | LifecycleState::Hibernated
            ) {
                self.lifecycle_state = LifecycleState::Hibernated;
            } else {
                for status in exited {
                    sabine_runtime::report_error(
                        "window",
                        format!("the browser exited ({status}); restarting it"),
                    );
                }
                self.begin_recovery();
            }
        }
        if self.drive_recovery(event_loop) {
            return;
        }
        if self.drive_pending_suspend(event_loop) {
            return;
        }
        let loading_deadline = self.drive_loading();
        let tooltip_deadline = self.drive_tooltip();
        if let Some(deadline) = self.hibernate_commit_deadline {
            if Instant::now() >= deadline {
                self.commit_hibernate();
                return;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            return;
        }
        if self.drive_resize_paint(event_loop) {
            return;
        }
        if let Some(deadline) = self.hibernate_deadline {
            if self.has_hibernation_blockers() {
                self.hibernate_deadline = None;
                return;
            }
            if Instant::now() >= deadline {
                self.begin_hibernate("idle");
                if let Some(deadline) = self.hibernate_commit_deadline {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                }
                return;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            return;
        }
        if self.cef_handed_off && self.socket.is_none() {
            let deadline = *self.handoff_deadline.get_or_insert_with(|| {
                Instant::now() + Duration::from_secs(HANDOFF_CONNECT_TIMEOUT_SECS)
            });
            if Instant::now() >= deadline {
                self.fail(format!("The shared browser did not connect within {HANDOFF_CONNECT_TIMEOUT_SECS} seconds."));
                self.force_close(event_loop);
                return;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                loading_deadline
                    .unwrap_or_else(|| Instant::now() + Duration::from_millis(250))
                    .min(deadline),
            ));
            return;
        }
        if self.cef_handed_off && self.socket.is_some() {
            self.handoff_deadline = None;
        }
        if let Some(deadline) = loading_deadline.into_iter().chain(tooltip_deadline).min() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
    }
}

/// CEF_RESULT_CODE_NORMAL_EXIT_PROCESS_NOTIFIED — second process for the same
/// root_cache_path notified the primary and exited.
const CEF_RESULT_CODE_NORMAL_EXIT_PROCESS_NOTIFIED: i32 = 24;
const HANDOFF_CONNECT_TIMEOUT_SECS: u64 = 15;
