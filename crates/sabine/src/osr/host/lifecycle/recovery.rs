use std::io::Write;
use std::time::{Duration, Instant};

use crate::osr::host::{native::OsrNativeHost, types::LifecycleState};
use crate::window::{PAGE_LOST_LINE, PAGE_STARTED_LINE};

impl OsrNativeHost {
    pub(in crate::osr::host) fn fail(&mut self, message: String) {
        if self.failure.is_none() {
            sabine_runtime::report_error("osr", &message);
            self.failure = Some(message);
            self.proxy.wake_up();
        }
    }

    pub(in crate::osr::host) fn begin_recovery(&mut self) {
        if self.closing_deadline.is_some()
            || self.failure.is_some()
            || self.awaiting_connection
            || self.recovery_deadline.is_some()
        {
            return;
        }
        self.report_page_lost();
        if self.lifecycle_state == LifecycleState::Frozen {
            self.lifecycle_state = LifecycleState::Suspended;
        }
        let now = Instant::now();
        while self
            .recoveries
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(60))
        {
            self.recoveries.pop_front();
        }
        if self.recoveries.len() >= 3 {
            self.fail("The browser stopped repeatedly. Close the application and try again. See the CEF and OSR logs for details.".to_string());
            return;
        }
        self.recoveries.push_back(now);
        let delay = Duration::from_millis(250 * 4u64.pow(self.recoveries.len() as u32 - 1));
        self.recovery_deadline = Some(now + delay);
        if self.config.visible {
            self.start_loading();
        }
        self.proxy.wake_up();
    }

    /// Starts the browser again once its restart is due, and gives up on a
    /// browser that never connects.
    pub(in crate::osr::host) fn drive_recovery(&mut self) -> Option<Instant> {
        if let Some(deadline) = self.recovery_deadline {
            if Instant::now() < deadline {
                return Some(deadline);
            }
            self.recovery_deadline = None;
            self.launch_child();
        }
        let deadline = self
            .connection_deadline
            .filter(|_| self.awaiting_connection)?;
        if Instant::now() < deadline {
            return Some(deadline);
        }
        self.fail("The browser did not connect to its window within 30 seconds.".to_string());
        None
    }

    /// Tells the app the page's document went away without the page leaving
    /// it, so what it was handed at startup goes to the document after it.
    pub(in crate::osr::host) fn report_page_lost(&self) {
        report_page(PAGE_LOST_LINE);
    }

    pub(in crate::osr::host) fn report_page_started(&self) {
        report_page(PAGE_STARTED_LINE);
    }
}

fn report_page(line: &str) {
    let mut output = std::io::stdout().lock();
    let _ = writeln!(output, "{line}");
    let _ = output.flush();
}
