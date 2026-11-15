use sabine_bridge::WindowControl;
use sabine_platform::{Display, WindowRegions};

use super::{BridgeEventEmitter, lock, regions_control, square_corners_control};
use crate::host::WindowId;

impl BridgeEventEmitter {
    /// Shows or hides the app's window: the one focused last, or the first
    /// one open.
    pub fn set_visible(&self, visible: bool) -> bool {
        self.front_control("visible", if visible { "1" } else { "0" })
    }

    /// Shows the app's window: the one focused last, or the first one open.
    pub fn show(&self) -> bool {
        self.front_control("show", "1")
    }

    /// Hides the app's window: the one focused last, or the first one open.
    pub fn hide(&self) -> bool {
        self.front_control("hide", "1")
    }

    pub(crate) fn quit(&self) -> bool {
        self.host_control(None, "quit", "1")
    }

    /// Shows and focuses the app's window: the one focused last, or the
    /// first one open.
    pub fn focus_window(&self) -> bool {
        self.focus_window_with_activation_token(None)
    }

    pub fn show_window(&self, window: WindowId) -> bool {
        self.host_control(Some(window), "show", "1")
    }

    pub fn hide_window(&self, window: WindowId) -> bool {
        self.host_control(Some(window), "hide", "1")
    }

    /// Shows and focuses one window. On Wayland, pass the activation token
    /// from the click or launch that asked for it, so the window may take
    /// focus.
    pub fn activate_window(&self, window: WindowId, activation_token: Option<&str>) -> bool {
        #[cfg(target_os = "windows")]
        crate::desktop::allow_foreground(window);
        self.host_control(
            Some(window),
            "focus",
            activation_token
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .unwrap_or("1"),
        )
    }

    /// Closes one window as its close button does, without asking
    /// [`SabineWindow::on_close_requested`](crate::SabineWindow::on_close_requested).
    /// A window built with `hide_on_close` hides instead.
    pub fn close_window(&self, window: WindowId) -> bool {
        self.host_control(Some(window), "close", "1")
    }

    /// Changes one window, such as its title, size or whether it stays above
    /// other windows. A change the desktop cannot make is reported to the
    /// app's diagnostics.
    pub fn control_window(&self, window: WindowId, control: &WindowControl) -> bool {
        serde_json::to_string(control)
            .is_ok_and(|control| self.host_control(Some(window), "window", &control))
    }

    /// The displays connected to the computer, as the app's windows last saw
    /// them. Empty until the app's first window opens.
    pub fn displays(&self) -> Vec<Display> {
        lock(&self.displays).clone()
    }

    /// Keeps the displays a window reported. Returns whether they changed.
    pub(crate) fn update_displays(&self, displays: &[Display]) -> bool {
        let mut current = lock(&self.displays);
        if *current == displays {
            return false;
        }
        *current = displays.to_vec();
        true
    }

    /// Replaces the blur, opaque and input regions of every window, for
    /// example to drop a sidebar's blur while the sidebar is hidden.
    pub fn set_regions(&self, regions: &WindowRegions) -> bool {
        self.deliver(None, regions_control(regions), None)
    }

    /// Replaces the blur, opaque and input regions of one window.
    pub fn set_regions_of(&self, window: WindowId, regions: &WindowRegions) -> bool {
        self.deliver(Some(window), regions_control(regions), None)
    }

    /// Squares the corners of every window's regions even while it floats,
    /// for desktops where people turned rounded window corners off. Windows
    /// opened later follow it too. Windows that are maximized, fullscreen or
    /// tiled have square corners either way.
    pub fn set_square_corners(&self, square: bool) -> bool {
        let mut current = lock(&self.square_corners);
        *current = square;
        self.deliver(None, square_corners_control(square), None)
    }

    /// Shows and focuses the app's window, the one focused last or the first
    /// one open, with the activation token Wayland asks for.
    pub fn focus_window_with_activation_token(&self, token: Option<&str>) -> bool {
        let front = lock(&self.targets).front();
        front.is_some_and(|window| self.activate_window(window, token))
    }
}
