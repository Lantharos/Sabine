use std::time::{Duration, Instant};

use winit::event_loop::ActiveEventLoop;

use crate::osr::host::native::OsrNativeHost;
use crate::render::{GpuRenderer, RendererError};

/// While a graphics driver restarts, its adapters can be missing for a few
/// seconds, so the window keeps looking for one instead of giving up.
const RECONNECT_INTERVAL: Duration = Duration::from_millis(250);
const RECONNECT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy)]
pub(in crate::osr::host) struct GpuReconnect {
    since: Instant,
    next_attempt: Instant,
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn recover_gpu(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.closing_deadline.is_some() || self.failure.is_some() {
            return;
        }
        if let Some(reason) = self.renderer.as_ref().and_then(GpuRenderer::device_loss) {
            self.lose_gpu(reason);
        }
        self.reconnect_gpu(event_loop);
    }

    /// When the window next tries to find a graphics device again, unless
    /// it is looking for one now.
    pub(in crate::osr::host) fn gpu_reconnect_deadline(&self) -> Option<Instant> {
        self.gpu_reconnect
            .filter(|_| !self.renderer_starting)
            .map(|reconnect| reconnect.next_attempt)
    }

    fn lose_gpu(&mut self, reason: String) {
        let now = Instant::now();
        while self
            .gpu_recoveries
            .front()
            .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(60))
        {
            self.gpu_recoveries.pop_front();
        }
        if self.gpu_recoveries.len() >= 3 {
            self.fail(format!("The graphics device stopped repeatedly. Restart the application. GPU details: {reason}"));
            return;
        }
        self.gpu_recoveries.push_back(now);
        sabine_runtime::report_error("gpu", format!("Recreating the graphics device: {reason}"));
        self.renderer = None;
        self.gpu_reconnect = Some(GpuReconnect {
            since: now,
            next_attempt: now,
        });
    }

    fn reconnect_gpu(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(reconnect) = self.gpu_reconnect else {
            return;
        };
        if self.renderer_starting || Instant::now() < reconnect.next_attempt {
            return;
        }
        if self.window.is_none() {
            self.gpu_reconnect = None;
            return;
        }
        self.start_renderer(event_loop, true);
    }

    /// Looks for a graphics device again a little later, while a restarting
    /// driver may still bring its adapters back.
    pub(in crate::osr::host) fn gpu_reconnect_failed(&mut self, error: RendererError) {
        let Some(reconnect) = self.gpu_reconnect else {
            return;
        };
        let now = Instant::now();
        if now.duration_since(reconnect.since) < RECONNECT_TIMEOUT {
            crate::osr::host::trace_host(&self.config, format!("gpu.reconnect.waiting {error}"));
            self.gpu_reconnect = Some(GpuReconnect {
                next_attempt: now + RECONNECT_INTERVAL,
                ..reconnect
            });
        } else {
            self.gpu_reconnect = None;
            self.fail(format!("Could not restore GPU rendering: {error}"));
        }
    }
}
