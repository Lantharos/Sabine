use std::time::{Duration, Instant};

use crate::osr::host::native::OsrNativeHost;
use crate::render::GpuRenderer;

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
    pub(in crate::osr::host) fn recover_gpu(&mut self) {
        if self.closing_deadline.is_some() || self.failure.is_some() {
            return;
        }
        if let Some(reason) = self.renderer.as_ref().and_then(GpuRenderer::device_loss) {
            self.lose_gpu(reason);
        }
        self.reconnect_gpu();
    }

    /// When the window next tries to find a graphics device again.
    pub(in crate::osr::host) fn gpu_reconnect_deadline(&self) -> Option<Instant> {
        self.gpu_reconnect.map(|reconnect| reconnect.next_attempt)
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

    fn reconnect_gpu(&mut self) {
        let Some(reconnect) = self.gpu_reconnect else {
            return;
        };
        let now = Instant::now();
        if now < reconnect.next_attempt {
            return;
        }
        let Some(window) = self.window.clone() else {
            self.gpu_reconnect = None;
            return;
        };
        let proxy = self.proxy.clone();
        match pollster::block_on(GpuRenderer::after_device_loss(
            window,
            self.config.transparent,
            move || proxy.wake_up(),
        )) {
            Ok(renderer) => {
                self.gpu_reconnect = None;
                self.resume_gpu(renderer);
            }
            Err(error) if now.duration_since(reconnect.since) < RECONNECT_TIMEOUT => {
                crate::osr::host::trace_host(
                    &self.config,
                    format!("gpu.reconnect.waiting {error}"),
                );
                self.gpu_reconnect = Some(GpuReconnect {
                    next_attempt: now + RECONNECT_INTERVAL,
                    ..reconnect
                });
            }
            Err(error) => {
                self.gpu_reconnect = None;
                self.fail(format!("Could not restore GPU rendering: {error}"));
            }
        }
    }

    fn resume_gpu(&mut self, renderer: GpuRenderer) {
        #[cfg(windows)]
        let adapter = crate::osr::accel::adapter_luid(&renderer);
        #[cfg(target_os = "linux")]
        let imports_dmabuf = renderer.imports_dmabuf();
        self.renderer = Some(renderer);
        #[cfg(target_os = "linux")]
        if !imports_dmabuf {
            self.paint_in_software("the graphics device cannot import dma-bufs");
        }
        #[cfg(windows)]
        if self.browser_adapter.as_ref() != Some(&adapter) {
            sabine_runtime::report_error(
                "gpu",
                "The graphics adapter changed; restarting the browser on the new one",
            );
            self.relaunch_browser();
            return;
        }
        self.send_control("repaint\n");
    }
}
