use crate::osr::host::native::OsrNativeHost;

/// Whether the desktop maximized, fullscreened or tiled the window. Windows
/// in any of these states sit against screen edges, so their corners are
/// square.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::osr::host) struct ToplevelState {
    maximized: bool,
    fullscreen: bool,
    tiled: bool,
}

impl ToplevelState {
    pub(in crate::osr::host) fn floating(self) -> bool {
        !(self.maximized || self.fullscreen || self.tiled)
    }
}

impl OsrNativeHost {
    fn read_toplevel_state(&self) -> ToplevelState {
        let Some(window) = &self.window else {
            return ToplevelState::default();
        };
        ToplevelState {
            maximized: window.is_maximized(),
            fullscreen: window.fullscreen().is_some(),
            tiled: is_tiled(window.as_ref()),
        }
    }

    /// Follows the state the desktop configured the window with, squaring the
    /// window's regions while it is not floating.
    pub(in crate::osr::host) fn sync_toplevel_state(&mut self) {
        let state = self.read_toplevel_state();
        if state == self.toplevel {
            return;
        }
        if state.floating() != self.toplevel.floating() {
            self.effect_regions_dirty = true;
        }
        self.toplevel = state;
        self.send_toplevel_state();
    }

    pub(in crate::osr::host) fn send_toplevel_state(&self) {
        let ToplevelState {
            maximized,
            fullscreen,
            tiled,
        } = self.toplevel;
        self.send_control(format!(
            "toplevel\t{}\t{}\t{}\n",
            u8::from(maximized),
            u8::from(fullscreen),
            u8::from(tiled)
        ));
    }
}

#[cfg(target_os = "linux")]
fn is_tiled(window: &dyn winit::window::Window) -> bool {
    use winit::platform::wayland::WindowExtWayland;
    window.is_tiled()
}

#[cfg(not(target_os = "linux"))]
fn is_tiled(_window: &dyn winit::window::Window) -> bool {
    false
}
