use super::OsrNativeHost;
use crate::osr::control::Wheel;

impl OsrNativeHost {
    pub(in crate::osr::host) fn send_control(&self, line: impl Into<String>) {
        let Some(writer) = &self.control_writer else {
            return;
        };
        if let Err(error) = writer.send(line.into()) {
            sabine_runtime::report_error("window", format!("browser control send failed: {error}"));
        }
    }

    pub(in crate::osr::host) fn send_mouse_motion(&self, line: String) {
        let Some(writer) = &self.control_writer else {
            return;
        };
        if let Err(error) = writer.send_motion(line) {
            sabine_runtime::report_error("window", format!("pointer send failed: {error}"));
        }
    }

    pub(in crate::osr::host) fn send_wheel(&self, wheel: Wheel) {
        let Some(writer) = &self.control_writer else {
            return;
        };
        if let Err(error) = writer.send_wheel(wheel) {
            sabine_runtime::report_error("window", format!("wheel send failed: {error}"));
        }
    }

    pub(in crate::osr::host) fn send_touch_move(&self, id: i32, line: String) {
        let Some(writer) = &self.control_writer else {
            return;
        };
        if let Err(error) = writer.send_touch_move(id, line) {
            sabine_runtime::report_error("window", format!("touch send failed: {error}"));
        }
    }

    pub(in crate::osr::host) fn send_pen_move(&self, line: String) {
        let Some(writer) = &self.control_writer else {
            return;
        };
        if let Err(error) = writer.send_pen_move(line) {
            sabine_runtime::report_error("window", format!("pen send failed: {error}"));
        }
    }

    /// Gives the browser back the slot of an accelerated frame the window
    /// leaves undrawn.
    pub(in crate::osr::host) fn release_accel_slot(&self, token: u64) {
        if let Some(writer) = &self.control_writer {
            writer.release_accel(token);
        }
    }
}
