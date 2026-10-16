use std::time::{Duration, Instant};

use crate::osr::host::native::OsrNativeHost;

const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(10);
const SLOW_LOAD_NOTICE: Duration = Duration::from_secs(30);

impl OsrNativeHost {
    pub(in crate::osr::host) fn note_main_load_ready(&mut self) {
        if let Some(loading) = &mut self.loading {
            loading.ready_at.get_or_insert_with(Instant::now);
        }
    }

    pub(in crate::osr::host) fn note_rejected_main_frame(&mut self, size: (u32, u32)) {
        if let Some(loading) = &mut self.loading {
            loading.rejected_frame = Some(size);
        }
    }

    /// Keeps the loading screen from hiding a page that never shows: a page
    /// that loaded without a usable frame switches to software painting, and
    /// fails with a diagnostic once software painting does not show it either.
    pub(in crate::osr::host) fn watch_first_frame(&mut self) -> Option<Instant> {
        let loading = self.loading?;
        let now = Instant::now();
        let Some(ready_at) = loading.ready_at else {
            return self.watch_slow_load(loading.started, loading.slow_load_reported);
        };
        if !self.accepts_paint() {
            self.loading.as_mut()?.ready_at = Some(now);
            return Some(now + FIRST_FRAME_TIMEOUT);
        }
        let deadline = ready_at + FIRST_FRAME_TIMEOUT;
        if now < deadline {
            return Some(deadline);
        }
        let reason = self.first_frame_stall();
        self.loading = None;
        self.recover_first_frame(reason);
        None
    }

    fn watch_slow_load(&mut self, started: Instant, reported: bool) -> Option<Instant> {
        let notice_at = started + SLOW_LOAD_NOTICE;
        if reported {
            return None;
        }
        if Instant::now() < notice_at {
            return Some(notice_at);
        }
        self.loading.as_mut()?.slow_load_reported = true;
        sabine_runtime::report_error(
            "window",
            format!(
                "The page has not finished loading after {} seconds",
                SLOW_LOAD_NOTICE.as_secs()
            ),
        );
        None
    }

    fn first_frame_stall(&self) -> String {
        let (width, height) = self.content_surface_size();
        let expected = format!(
            "the window needs a {width}x{height} page at scale {:.2}",
            self.scale()
        );
        match self.loading.and_then(|loading| loading.rejected_frame) {
            Some((frame_width, frame_height)) => {
                format!("the browser painted {frame_width}x{frame_height} pixels, but {expected}")
            }
            None => format!("the browser sent no frame, and {expected}"),
        }
    }

    #[cfg(target_os = "linux")]
    fn recover_first_frame(&mut self, reason: String) {
        if self.accelerated_launch && !self.software_paint {
            self.paint_in_software(&format!(
                "the page loaded {} seconds ago but never showed: {reason}",
                FIRST_FRAME_TIMEOUT.as_secs()
            ));
        } else {
            self.fail_first_frame(&reason);
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn recover_first_frame(&mut self, reason: String) {
        self.fail_first_frame(&reason);
    }

    fn fail_first_frame(&mut self, reason: &str) {
        self.fail(format!(
            "The page loaded but its window stayed blank for {} seconds: {reason}.",
            FIRST_FRAME_TIMEOUT.as_secs()
        ));
    }
}
