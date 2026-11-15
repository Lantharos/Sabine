use std::sync::Arc;

use sabine_bridge::{WindowControl, WindowSize};
use winit::dpi::LogicalSize;
use winit::monitor::Fullscreen;
#[cfg(not(target_os = "linux"))]
use winit::window::WindowLevel;

use crate::osr::host::native::OsrNativeHost;

/// The window's title: the one the app named it, or else its page's
/// `document.title`, or else the app's title while the page has none.
#[derive(Debug)]
pub(in crate::osr::host) struct WindowTitle {
    named: Option<Arc<str>>,
    page: Option<Arc<str>>,
    app: Arc<str>,
}

impl WindowTitle {
    pub(in crate::osr::host) fn new(app: Arc<str>) -> Self {
        Self {
            named: None,
            page: None,
            app,
        }
    }

    pub(in crate::osr::host) fn shown(&self) -> &Arc<str> {
        self.named
            .as_ref()
            .or(self.page.as_ref())
            .unwrap_or(&self.app)
    }

    /// Returns whether the shown title changed.
    fn update(&mut self, change: impl FnOnce(&mut Self)) -> bool {
        let before = Arc::clone(self.shown());
        change(self);
        before != *self.shown()
    }
}

impl OsrNativeHost {
    /// Changes the window for someone who waits for no answer, reporting a
    /// change the desktop cannot make.
    pub(in crate::osr::host) fn control_window(&mut self, control: WindowControl) {
        if let Err(error) = self.apply_window_control(control) {
            sabine_runtime::report_error("window", error);
        }
    }

    pub(in crate::osr::host) fn apply_window_control(
        &mut self,
        control: WindowControl,
    ) -> Result<(), String> {
        match control {
            WindowControl::SetTitle { title } => {
                self.retitle(|title_state| {
                    title_state.named = title.map(Arc::from);
                });
            }
            WindowControl::SetSize { width, height } => self.resize_window(width, height),
            WindowControl::SetMinSize { size } => {
                let min = size.unwrap_or(WindowSize::new(0, 0));
                self.config.min_width = min.width;
                self.config.min_height = min.height;
                if let Some(window) = &self.window {
                    window.set_min_surface_size(size.map(|size| logical(size).into()));
                }
            }
            WindowControl::SetMaxSize { size } => {
                self.config.max_size = size;
                if let Some(window) = &self.window {
                    window.set_max_surface_size(size.map(|size| logical(size).into()));
                }
            }
            #[cfg(not(target_os = "linux"))]
            WindowControl::SetPosition { x, y } => {
                if let Some(window) = &self.window {
                    window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y).into());
                }
            }
            #[cfg(target_os = "linux")]
            WindowControl::SetPosition { .. } => {
                return Err("Wayland places windows itself, so an app can't move them".into());
            }
            WindowControl::SetAlwaysOnTop { enabled } => self.keep_above(enabled)?,
            WindowControl::SetResizable { enabled } => {
                self.config.resizable = enabled;
                if let Some(window) = &self.window {
                    window.set_resizable(enabled);
                }
            }
            WindowControl::Minimize => self.minimize_window(),
            WindowControl::Maximize | WindowControl::Unmaximize => {
                let maximized = matches!(control, WindowControl::Maximize);
                self.config.maximized = maximized;
                if let Some(window) = &self.window {
                    window.set_maximized(maximized);
                }
            }
            WindowControl::SetFullscreen { enabled } => {
                if let Some(window) = &self.window {
                    window.set_fullscreen(enabled.then_some(Fullscreen::Borderless(None)));
                }
            }
        }
        Ok(())
    }

    /// Follows the page's `document.title`, unless the app named the window.
    pub(in crate::osr::host) fn follow_page_title(&mut self, title: String) {
        let title = title.trim();
        self.retitle(|title_state| {
            title_state.page = (!title.is_empty()).then(|| Arc::from(title));
        });
    }

    fn retitle(&mut self, change: impl FnOnce(&mut WindowTitle)) {
        if !self.title.update(change) {
            return;
        }
        if let Some(window) = &self.window {
            window.set_title(self.title.shown());
            window.request_redraw();
        }
    }

    fn resize_window(&mut self, width: u32, height: u32) {
        self.config.width = width.max(self.config.min_width);
        self.config.height = height.max(self.config.min_height);
        let Some(window) = self.window.clone() else {
            return;
        };
        let size = WindowSize::new(self.config.width, self.config.height);
        if let Some(resized) = window.request_surface_size(logical(size).into()) {
            self.surface_resized(resized);
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn keep_above(&mut self, enabled: bool) -> Result<(), String> {
        self.config.always_on_top = enabled;
        if let Some(window) = &self.window {
            window.set_window_level(if enabled {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            });
        }
        Ok(())
    }

    /// Wayland keeps windows above others only through `kestrel_window_v1`.
    #[cfg(target_os = "linux")]
    fn keep_above(&mut self, enabled: bool) -> Result<(), String> {
        if let Some(placement) = &self.placement {
            placement.set_keep_above(enabled);
        } else if let Some(window) = self.window.as_ref().filter(|_| enabled) {
            let placement = sabine_platform::WindowPlacement::new(
                window.as_ref(),
                self.config.skip_taskbar,
                true,
            )?;
            self.placement = Some(placement);
        }
        self.config.always_on_top = enabled;
        Ok(())
    }
}

fn logical(size: WindowSize) -> LogicalSize<f64> {
    LogicalSize::new(f64::from(size.width), f64::from(size.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn title() -> WindowTitle {
        WindowTitle::new(Arc::from("Notes"))
    }

    #[test]
    fn the_page_title_names_the_window_until_the_app_does() {
        let mut title = title();
        assert!(title.update(|title| title.page = Some(Arc::from("Groceries"))));
        assert_eq!(&**title.shown(), "Groceries");
        assert!(title.update(|title| title.named = Some(Arc::from("Inbox"))));
        assert!(!title.update(|title| title.page = Some(Arc::from("Recipes"))));
        assert_eq!(&**title.shown(), "Inbox");
        assert!(title.update(|title| title.named = None));
        assert_eq!(&**title.shown(), "Recipes");
    }

    #[test]
    fn a_page_without_a_title_shows_the_apps() {
        let mut title = title();
        title.update(|title| title.page = Some(Arc::from("Groceries")));
        assert!(title.update(|title| title.page = None));
        assert_eq!(&**title.shown(), "Notes");
    }
}
