use std::time::{Duration, Instant};

use winit::event_loop::{ActiveEventLoop, ControlFlow};

use crate::osr::host::{
    native::OsrNativeHost,
    types::{LifecycleState, NativeLoading},
};

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
            self.loading = Some(NativeLoading::new());
        }
        self.proxy.wake_up();
    }

    pub(in crate::osr::host) fn drive_recovery(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
    ) -> bool {
        if self.failure.is_some() {
            self.force_close(event_loop);
            return true;
        }
        if let Some(deadline) = self.recovery_deadline {
            if Instant::now() >= deadline {
                self.recovery_deadline = None;
                self.launch_child();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                return true;
            }
        }
        if self.awaiting_connection
            && let Some(deadline) = self.connection_deadline
        {
            if Instant::now() >= deadline {
                self.fail(
                    "The browser did not connect to its window within 30 seconds.".to_string(),
                );
                self.force_close(event_loop);
                return true;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                deadline.min(Instant::now() + Duration::from_millis(100)),
            ));
        }
        false
    }
}
