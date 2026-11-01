use std::time::Instant;

use winit::event_loop::{ActiveEventLoop, ControlFlow};

use crate::osr::host::native::OsrNativeHost;

impl OsrNativeHost {
    /// Runs whatever is due, and sleeps until the soonest thing that is not.
    pub(in crate::osr::host) fn drive_deadlines(&mut self, event_loop: &dyn ActiveEventLoop) {
        let next = self.next_deadline(event_loop);
        event_loop.set_control_flow(next.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn next_deadline(&mut self, event_loop: &dyn ActiveEventLoop) -> Option<Instant> {
        if let Some(deadline) = self.closing_deadline {
            if Instant::now() < deadline {
                return Some(deadline);
            }
            self.force_close(event_loop);
            return None;
        }
        let connection = [
            self.drive_recovery(),
            self.drive_handoff(),
            self.drive_incompatible_browser(),
        ];
        if self.failure.is_some() {
            self.force_close(event_loop);
            return None;
        }
        connection
            .into_iter()
            .chain([
                self.drive_pending_suspend(),
                self.drive_loading(),
                self.drive_tooltip(),
                self.watch_first_frame(),
                self.watch_reveal_hold(),
                self.drive_resize_paint(),
                self.drive_freeze(),
                self.gpu_reconnect_deadline(),
            ])
            .flatten()
            .min()
    }

    fn drive_freeze(&mut self) -> Option<Instant> {
        self.refresh_freeze();
        let deadline = self.freeze_deadline?;
        if Instant::now() < deadline {
            return Some(deadline);
        }
        self.freeze();
        None
    }
}
