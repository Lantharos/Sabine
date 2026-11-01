use std::time::{Duration, Instant};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{LoadingFade, NativeLoading, RELOAD_SPLASH_DELAY, REVEAL_HOLD_LIMIT};

impl OsrNativeHost {
    /// Waits for the page while a window on screen keeps its last frame, and
    /// shows the loading screen after its delay when the app has one.
    pub(in crate::osr::host) fn start_loading(&mut self) {
        self.loading = Some(NativeLoading::new(self.splash_delay()));
        self.loading_fade = None;
    }

    fn splash_delay(&self) -> Option<Duration> {
        let launch = self.config.splash?;
        Some(if self.presented {
            RELOAD_SPLASH_DELAY
        } else {
            launch
        })
    }

    /// Whether the window keeps showing its last frame while the page loads.
    pub(in crate::osr::host) fn holds_last_frame(&self) -> bool {
        self.presented && self.loading.is_some_and(|loading| !loading.splash_shown())
    }

    /// Shows the page, fading out the loading screen if it was on screen.
    pub(in crate::osr::host) fn finish_loading(&mut self) {
        let Some(loading) = self.loading.take() else {
            return;
        };
        if self.presented && loading.splash_shown() {
            self.loading_fade = Some(LoadingFade {
                loading,
                started: Instant::now(),
            });
        }
    }

    /// Whether the page has loaded and no longer holds back its first frame.
    pub(in crate::osr::host) fn page_ready(&self) -> bool {
        self.main_load_ready && self.reveal_held_since.is_none()
    }

    /// Keeps the page's frames off screen until it calls `reveal_page`, for
    /// pages that still fetch what their first view shows after loading.
    pub(in crate::osr::host) fn hold_reveal(&mut self) {
        if !self.main_frame_presented {
            crate::osr::host::trace_host(&self.config, "browser.reveal_held");
            self.reveal_held_since = Some(Instant::now());
        }
    }

    /// Shows the page with the next frame it paints, which has its first view.
    pub(in crate::osr::host) fn reveal_page(&mut self) {
        if self.reveal_held_since.take().is_some() {
            crate::osr::host::trace_host(&self.config, "browser.reveal");
            self.send_control("repaint\n");
        }
    }

    /// Shows a page that held back its first frame for too long.
    pub(in crate::osr::host) fn watch_reveal_hold(&mut self) -> Option<Instant> {
        let deadline = self.reveal_held_since? + REVEAL_HOLD_LIMIT;
        if Instant::now() < deadline {
            return Some(deadline);
        }
        sabine_runtime::report_error(
            "window",
            format!(
                "The page held back its first frame for {} seconds without revealing it",
                REVEAL_HOLD_LIMIT.as_secs()
            ),
        );
        self.reveal_page();
        None
    }

    /// Ends a finished fade and keeps drawing frames while one runs.
    pub(in crate::osr::host) fn drive_loading_fade(&mut self) {
        let Some(fade) = self.loading_fade else {
            return;
        };
        if fade.opacity().is_none() {
            self.loading_fade = None;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
