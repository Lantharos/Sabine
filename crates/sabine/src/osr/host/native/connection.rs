use std::{sync::mpsc, time::Instant};

use winit::event_loop::{ActiveEventLoop, EventLoopProxy};

use super::OsrNativeHost;
use crate::bridge::frame::Frame;
use crate::osr::control::ControlRelay;
use crate::osr::host::types::{CLOSE_GRACE, HostControl, OsrHostEvent};

impl OsrNativeHost {
    pub(in crate::osr::host) fn begin_close(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.closing_deadline.is_some() {
            return;
        }
        if let Some(window) = &self.window {
            window.set_visible(false);
        }
        self.send_control("close\n");
        self.closing_deadline = Some(Instant::now() + CLOSE_GRACE);
        if self.browser_gone() {
            self.force_close(event_loop);
        }
    }

    pub(in crate::osr::host) fn force_close(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.drop_connection();
        event_loop.exit();
    }

    pub(in crate::osr::host) fn drop_connection(&mut self) {
        if let Some(socket) = self.socket.take() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        self.socket_reader = None;
        self.relay.connect(None);
        self.control_writer = None;
        self.pending_messages = None;
        #[cfg(target_os = "macos")]
        {
            self.surface_broker = None;
        }
    }
}

/// Reads what the app sends the window. The window closes once the app's end
/// closes, as it does when the app crashes.
pub(super) fn start_parent_bridge_reader(
    sender: mpsc::SyncSender<OsrHostEvent>,
    proxy: EventLoopProxy,
    relay: ControlRelay,
) {
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        while let Ok(Some(frame)) = Frame::read(&mut input) {
            if frame.body.is_none()
                && let Some((command, value)) = crate::bridge::parse_host_control(&frame.line)
                && let Some(control) =
                    crate::osr::host::events::host_control_from_parts(command, value)
            {
                if sender.send(OsrHostEvent::HostControl(control)).is_err() {
                    break;
                }
                proxy.wake_up();
                continue;
            }
            if relay.forward(frame.to_bytes()) {
                if sender.send(OsrHostEvent::FrozenPageMessaged).is_err() {
                    break;
                }
                proxy.wake_up();
            }
        }
        if sender
            .send(OsrHostEvent::HostControl(HostControl::Quit))
            .is_ok()
        {
            proxy.wake_up();
        }
    });
}
