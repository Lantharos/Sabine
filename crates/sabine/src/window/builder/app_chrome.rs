use sabine_platform::WindowRegion;
#[cfg(not(target_os = "macos"))]
use sabine_platform::WindowRegionRect;

use super::SabineWindow;
#[cfg(not(target_os = "macos"))]
use super::config::SabineWindowControlAction;

/// App-drawn chrome layout: titlebar drag strip, optional sidebar glass regions,
/// and the window controls. On macOS the system's own traffic lights sit over
/// the page's titlebar; elsewhere the page draws minimize, maximize and close
/// at the titlebar's right end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppChrome {
    pub titlebar: i32,
    pub sidebar: i32,
    pub radius: i32,
    pub control_width: i32,
}

impl Default for AppChrome {
    fn default() -> Self {
        Self {
            titlebar: 38,
            sidebar: 260,
            radius: 14,
            control_width: 46,
        }
    }
}

impl AppChrome {
    pub fn new(titlebar: i32, sidebar: i32) -> Self {
        Self {
            titlebar,
            sidebar,
            ..Self::default()
        }
    }

    pub fn titlebar_only(titlebar: i32) -> Self {
        Self {
            titlebar,
            sidebar: 0,
            radius: 0,
            ..Self::default()
        }
    }
}

impl SabineWindow {
    /// Sets drag/control regions and optional blur/opaque/input regions for an
    /// app-drawn titlebar (and optional sidebar glass layout). Pages find the
    /// corner the system's controls cover through `appWindow.controlsOverlay`.
    pub fn app_chrome(self, chrome: AppChrome) -> Self {
        let titlebar = chrome.titlebar.max(0);
        let sidebar = chrome.sidebar.max(0);
        let window = self.titlebar_drag_region(titlebar).window_controls(chrome);
        if sidebar == 0 {
            return window;
        }
        let radius = window.window_corner_radius(chrome);
        window
            .blur_region(WindowRegion::adaptive_titlebar_sidebar(
                sidebar, titlebar, radius,
            ))
            .opaque_region(WindowRegion::adaptive_content_after_sidebar(
                sidebar, titlebar,
            ))
            .rounded_input_region(chrome)
    }

    #[cfg(target_os = "macos")]
    fn window_controls(mut self, _chrome: AppChrome) -> Self {
        self.config.chrome = super::SabineWindowChrome::System;
        self.config.titlebar_overlay = true;
        self
    }

    #[cfg(not(target_os = "macos"))]
    fn window_controls(self, chrome: AppChrome) -> Self {
        let titlebar = chrome.titlebar.max(0);
        let control = chrome.control_width.max(1);
        [
            SabineWindowControlAction::Minimize,
            SabineWindowControlAction::Maximize,
            SabineWindowControlAction::Close,
        ]
        .into_iter()
        .zip([3, 2, 1])
        .fold(self, |window, (action, slot)| {
            window.control_region(
                action,
                WindowRegionRect::new(-(control * slot), 0, control, titlebar),
            )
        })
    }

    /// AppKit rounds and clips the window itself, so regions keep square
    /// corners there.
    fn window_corner_radius(&self, chrome: AppChrome) -> i32 {
        if self.config.titlebar_overlay {
            0
        } else {
            chrome.radius.max(0)
        }
    }

    fn rounded_input_region(self, chrome: AppChrome) -> Self {
        match self.window_corner_radius(chrome) {
            0 => self,
            radius => self.input_region(WindowRegion::adaptive_rounded_rect(radius)),
        }
    }
}
