use std::io::Write;

use winit::event_loop::ActiveEventLoop;

use super::Flow;
use crate::osr::host::native::OsrNativeHost;
use crate::window::CLOSE_REQUESTED_LINE;

impl OsrNativeHost {
    /// Someone asked the window to close, with its close button or through
    /// the desktop. An app that answers such requests hears of it and closes
    /// the window itself.
    pub(in crate::osr::host) fn close_requested(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.config.asks_before_closing {
            let mut output = std::io::stdout().lock();
            let _ = writeln!(output, "{CLOSE_REQUESTED_LINE}");
            let _ = output.flush();
        } else {
            self.close_window(event_loop);
        }
    }

    /// Closes the window, or hides one that hides when it closes.
    pub(super) fn close_window(&mut self, event_loop: &dyn ActiveEventLoop) -> Flow {
        self.remember_size();
        if self.config.hide_on_close {
            self.hide_window("close");
            Flow::Continue
        } else {
            self.forget_state();
            self.begin_close(event_loop);
            Flow::Closed
        }
    }
}
