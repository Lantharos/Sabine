use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

use winit::event_loop::ActiveEventLoop;

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::OsrHostEvent;

/// CEF_RESULT_CODE_NORMAL_EXIT_PROCESS_NOTIFIED — second process for the same
/// root_cache_path notified the primary and exited.
const CEF_RESULT_CODE_NORMAL_EXIT_PROCESS_NOTIFIED: i32 = 24;
const HANDOFF_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const HANDOFF_EXIT_TIMEOUT: Duration = Duration::from_secs(1);

impl OsrNativeHost {
    /// Hears from a thread waiting on the window's own Chromium when it
    /// exits.
    pub(in crate::osr::host) fn watch_browser(&mut self, generation: u64, mut child: Child) {
        let sender = self.sender.clone();
        let proxy = self.proxy.clone();
        let watching = std::thread::Builder::new()
            .name("sabine-browser-exit".to_string())
            .spawn(move || {
                let Ok(status) = child.wait() else {
                    return;
                };
                if sender
                    .send(OsrHostEvent::BrowserExited(generation, status))
                    .is_ok()
                {
                    proxy.wake_up();
                }
            });
        match watching {
            Ok(_) => self.running_browsers.push(generation),
            Err(error) => self.fail(format!("Could not watch the browser: {error}")),
        }
    }

    pub(in crate::osr::host) fn browser_attached(&self) -> bool {
        self.socket.is_some() || self.awaiting_connection
    }

    /// Whether nothing of the window's browser is left to wait for.
    pub(in crate::osr::host) fn browser_gone(&self) -> bool {
        self.running_browsers.is_empty() && !self.browser_attached()
    }

    pub(in crate::osr::host) fn browser_exited(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        generation: u64,
        status: ExitStatus,
    ) {
        self.running_browsers
            .retain(|running| *running != generation);
        if self.closing_deadline.is_some() {
            if self.browser_gone() {
                self.force_close(event_loop);
            }
            return;
        }
        if generation != self.connection_generation {
            return;
        }
        let handed_off = status.code() == Some(CEF_RESULT_CODE_NORMAL_EXIT_PROCESS_NOTIFIED);
        if self.incompatible_since.take().is_some() {
            self.fail(self.incompatible_browser_message(handed_off));
            self.force_close(event_loop);
            return;
        }
        if handed_off {
            self.cef_handed_off = true;
            self.handoff_deadline = Some(Instant::now() + HANDOFF_CONNECT_TIMEOUT);
            crate::osr::host::trace_host(&self.config, "cef.handed_off.waiting_for_primary");
            return;
        }
        if self.socket.is_none() {
            self.awaiting_connection = false;
            sabine_runtime::report_error(
                "window",
                format!("the browser exited ({status}); restarting it"),
            );
            self.begin_recovery();
        }
    }

    /// Waits for the Chromium that holds the app's profile after the
    /// window's own one handed the window over to it.
    pub(in crate::osr::host) fn drive_handoff(&mut self) -> Option<Instant> {
        if !self.cef_handed_off {
            return None;
        }
        if self.socket.is_some() {
            self.handoff_deadline = None;
            return None;
        }
        let deadline = *self
            .handoff_deadline
            .get_or_insert_with(|| Instant::now() + HANDOFF_CONNECT_TIMEOUT);
        if Instant::now() < deadline {
            return Some(deadline);
        }
        self.fail(format!(
            "The shared browser did not connect within {} seconds.",
            HANDOFF_CONNECT_TIMEOUT.as_secs()
        ));
        None
    }

    /// Handles a browser that speaks another version of Sabine's protocol,
    /// and returns whether the window has to close. A shared runtime started
    /// from another build is left for the app's own Chromium. The app's own
    /// Chromium either is that browser, from a Sabine host the app does not
    /// match, or handed the window to the Chromium of another copy of the app
    /// that holds the app's profile. Chromium exits right after such a
    /// handoff, so the window waits briefly to tell which.
    pub(in crate::osr::host) fn refuse_incompatible_browser(&mut self) -> bool {
        self.drop_connection();
        self.awaiting_connection = false;
        #[cfg(target_os = "linux")]
        if self.browser_shared {
            sabine_runtime::report_error(
                "window",
                "the shared runtime runs another version of Sabine, so the app starts its own",
            );
            self.shared_runtime_refused = true;
            self.launch_child();
            return false;
        }
        if !self.cef_handed_off && self.running_browsers.contains(&self.connection_generation) {
            self.incompatible_since = Some(Instant::now());
            return false;
        }
        self.fail(self.incompatible_browser_message(self.cef_handed_off));
        true
    }

    pub(in crate::osr::host) fn drive_incompatible_browser(&mut self) -> Option<Instant> {
        let deadline = self.incompatible_since? + HANDOFF_EXIT_TIMEOUT;
        if Instant::now() < deadline {
            return Some(deadline);
        }
        self.incompatible_since = None;
        self.fail(self.incompatible_browser_message(false));
        None
    }

    fn incompatible_browser_message(&self, handed_off: bool) -> String {
        let name = &self.config.title;
        if handed_off {
            format!(
                "{name} is already open with another version of Sabine. Close it, then open {name} again."
            )
        } else {
            format!(
                "This version of {name} needs another version of Sabine. Update Sabine, then open {name} again."
            )
        }
    }
}
