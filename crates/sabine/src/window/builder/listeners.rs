use crate::window::SabineWindow;

impl SabineWindow {
    /// Calls `listener` with a handle to the app's process once it launches,
    /// for app code that presents or opens windows later, such as a
    /// background app answering a click on one of its notifications.
    pub fn on_launched(
        mut self,
        listener: impl Fn(crate::SabineProcessHandle) + Send + Sync + 'static,
    ) -> Self {
        self.config.launched = Some(crate::window::LaunchListener::new(listener));
        self
    }

    /// Calls `listener` before the app's process ends on its own: after its
    /// last window closed, when it quits, or when a background app hands over
    /// to the copy of it installed while it ran. The process ends once
    /// `listener` returns, so the app can finish or save what it is doing.
    pub fn on_quit(mut self, listener: impl Fn() + Send + Sync + 'static) -> Self {
        self.config.quit = Some(crate::window::QuitListener::new(listener));
        self
    }

    /// Calls `listener` with each tray click, global shortcut, later launch
    /// and link the desktop hands the app, before its pages hear of it. It
    /// runs whether or not a window is open, so an app built with
    /// [`Self::background`] can answer a tray click with
    /// [`SabineProcessHandle::present`](crate::SabineProcessHandle::present).
    pub fn on_desktop_event(
        mut self,
        listener: impl Fn(&crate::SabineProcessHandle, &sabine_platform::PlatformEvent)
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.config.desktop_events = Some(crate::window::DesktopEventListener::new(listener));
        self
    }

    /// Calls `listener` whenever the window becomes visible or hidden, or its
    /// page is suspended or resumed, starting with the state it opens in and
    /// ending with a hidden report when it closes.
    pub fn on_visibility_changed(
        mut self,
        listener: impl Fn(crate::window::WindowVisibility) + Send + Sync + 'static,
    ) -> Self {
        self.config.listeners.visibility = Some(crate::window::VisibilityListener::new(listener));
        self
    }

    /// Calls `listener` with the window's id when someone asks the window to
    /// close: with its close button, a shortcut such as Alt+F4, or from the
    /// taskbar. The window closes when it returns `true` and stays open when
    /// it returns `false`, for an app that asks first while work would be
    /// lost. Closing the window from its page or with
    /// [`BridgeEventEmitter::close_window`](crate::BridgeEventEmitter::close_window)
    /// doesn't ask.
    pub fn on_close_requested(
        mut self,
        listener: impl Fn(crate::WindowId) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.config.listeners.close_request =
            Some(crate::window::CloseRequestListener::new(listener));
        self
    }

    /// Calls `listener` with the window's id once the window has closed.
    pub fn on_closed(mut self, listener: impl Fn(crate::WindowId) + Send + Sync + 'static) -> Self {
        self.config.listeners.closed = Some(crate::window::ClosedListener::new(listener));
        self
    }

    /// Calls `listener` with the window's id whenever the desktop switches
    /// between light and dark or its accent color changes, starting with the
    /// appearance the window opens with.
    pub fn on_appearance_changed(
        mut self,
        listener: impl Fn(crate::WindowId, sabine_platform::Appearance) + Send + Sync + 'static,
    ) -> Self {
        self.config.listeners.appearance = Some(crate::window::AppearanceListener::new(listener));
        self
    }

    /// Calls `listener` with the displays connected to the computer whenever
    /// one is connected or disconnected, or changes its bounds, work area,
    /// scale or refresh rate, starting with the displays as the app's first
    /// window opens.
    pub fn on_displays_changed(
        mut self,
        listener: impl Fn(&[sabine_platform::Display]) + Send + Sync + 'static,
    ) -> Self {
        self.config.listeners.displays = Some(crate::window::DisplaysListener::new(listener));
        self
    }
}
