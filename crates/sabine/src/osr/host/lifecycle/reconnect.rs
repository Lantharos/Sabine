use winit::event_loop::ActiveEventLoop;

use crate::osr::host::input::InputPanelState;
use crate::osr::host::native::OsrNativeHost;

impl OsrNativeHost {
    /// The desktop's compositor went away. Everything made on its Wayland
    /// connection goes before the connection closes, while the page keeps
    /// running in the browser and its last frames stay with the window.
    pub(in crate::osr::host) fn lose_display(&mut self) {
        crate::osr::host::trace_host(&self.config, "display.lost");
        self.display_lost = true;
        self.drop_presented_window();
        self.media.disconnect();
        self.clipboard = None;
        self.input_panel = InputPanelState::Unconnected;
    }

    /// Opens the window again on the compositor that took over the display,
    /// with the page as it was.
    pub(in crate::osr::host) fn regain_display(&mut self, event_loop: &dyn ActiveEventLoop) {
        crate::osr::host::trace_host(&self.config, "display.regained");
        self.display_lost = false;
        self.connect_clipboard(event_loop);
        if self.config.visible {
            self.create_window(event_loop);
            self.send_resize();
        }
    }
}
