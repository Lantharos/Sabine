use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sabine_platform::{Display, DisplayWatcher};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};

use super::BridgeRequest;
use crate::osr::host::native::OsrNativeHost;
use crate::window::DISPLAYS_LINE;

/// The displays connected to the computer, read again whenever the desktop
/// says they changed.
pub(in crate::osr::host) struct Displays {
    current: Vec<Display>,
    changed: Arc<AtomicBool>,
    _watcher: DisplayWatcher,
}

impl Displays {
    pub(in crate::osr::host) fn watch(proxy: EventLoopProxy) -> Self {
        let changed = Arc::new(AtomicBool::new(true));
        let watched = Arc::clone(&changed);
        let watcher = DisplayWatcher::start(move || {
            watched.store(true, Ordering::Release);
            proxy.wake_up();
        });
        Self {
            current: Vec::new(),
            changed,
            _watcher: watcher,
        }
    }
}

impl OsrNativeHost {
    /// Watches the displays of the compositor that took over the display.
    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn watch_new_displays(&mut self) {
        self.displays = Displays::watch(self.proxy.clone());
    }

    /// Reads the displays again once the desktop said they changed, and tells
    /// the page and the app when they did.
    pub(in crate::osr::host) fn refresh_displays(&mut self, event_loop: &dyn ActiveEventLoop) {
        if !self.displays.changed.swap(false, Ordering::Acquire) {
            return;
        }
        let displays = sabine_platform::displays(event_loop);
        if displays == self.displays.current {
            return;
        }
        self.displays.current = displays;
        let json = serde_json::json!(self.displays.current);
        self.send_control(format!(
            "SABINE_BRIDGE_EVENT\t\"{}\"\t{json}\n",
            sabine_bridge::DISPLAYS_EVENT
        ));
        let mut output = std::io::stdout().lock();
        let _ = writeln!(output, "{DISPLAYS_LINE}\t{json}");
        let _ = output.flush();
    }

    pub(super) fn answer_displays(&self, request: &BridgeRequest) {
        self.send_bridge_response(
            request.browser_id,
            request.request_id,
            Ok(serde_json::json!(self.displays.current)),
        );
    }
}
