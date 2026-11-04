use winit::event_loop::ActiveEventLoop;

use super::OsrNativeHost;
use crate::osr::host::types::OsrHostEvent;
use crate::render::{GpuRenderer, RendererError};

impl OsrNativeHost {
    /// Graphics devices still starting, which end on their own thread when
    /// the host is gone. They have to end before the process exits.
    pub(in crate::osr::host) fn take_gpu_starts(&mut self) -> Vec<std::thread::JoinHandle<()>> {
        std::mem::take(&mut self.gpu_starts)
    }

    /// Starts the window's graphics device. On Linux, when the compositor
    /// can show the page's frames itself, the device starts once the first
    /// one is on screen instead.
    pub(in crate::osr::host) fn start_rendering(&mut self, event_loop: &dyn ActiveEventLoop) {
        #[cfg(target_os = "linux")]
        if self.start_direct_presentation() {
            return;
        }
        self.start_renderer(event_loop, false);
    }

    /// Starts the window's graphics device for software frames, which the
    /// compositor can't show itself. A window waiting to show its first
    /// frame directly would otherwise never start one, as no frame it could
    /// show is coming.
    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn draw_without_direct_presentation(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
    ) {
        if self.renderer.is_some() || self.renderer_starting {
            return;
        }
        self.direct = None;
        self.start_renderer(event_loop, false);
    }

    /// Starts the window's graphics device off the event loop, so the
    /// browser's connection, bridge calls and frames keep flowing while the
    /// drivers start. Only what the platform requires of the window's thread
    /// happens here.
    pub(in crate::osr::host) fn start_renderer(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        after_device_loss: bool,
    ) {
        if self.renderer.is_some() || self.renderer_starting {
            return;
        }
        let Some(window) = self.window.clone() else {
            return;
        };
        let generation = self.window_generation;
        let start = match GpuRenderer::prepare(window, after_device_loss) {
            Ok(start) => start,
            Err(error) => {
                self.renderer_started(event_loop, generation, Err(error));
                return;
            }
        };
        self.renderer_starting = true;
        let transparent = self.config.transparent;
        let sender = self.sender.clone();
        let proxy = self.proxy.clone();
        self.gpu_starts.retain(|start| !start.is_finished());
        let started = std::thread::Builder::new()
            .name("sabine-gpu-start".to_string())
            .spawn(move || {
                let wake = proxy.clone();
                let renderer =
                    pollster::block_on(GpuRenderer::start(start, transparent, move || {
                        wake.wake_up()
                    }));
                let event = OsrHostEvent::RendererStarted(generation, Box::new(renderer));
                if sender.send(event).is_ok() {
                    proxy.wake_up();
                }
            });
        match started {
            Ok(start) => self.gpu_starts.push(start),
            Err(error) => {
                self.fail(format!("Could not initialize GPU rendering: {error}"));
                event_loop.exit();
            }
        }
    }

    pub(in crate::osr::host) fn renderer_started(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        generation: u64,
        renderer: Result<GpuRenderer, RendererError>,
    ) {
        if generation != self.window_generation || self.window.is_none() {
            return;
        }
        self.renderer_starting = false;
        let renderer = match renderer {
            Ok(renderer) => renderer,
            Err(error) if self.gpu_reconnect.is_some() => {
                self.gpu_reconnect_failed(error);
                return;
            }
            Err(error) => {
                self.fail(format!("Could not initialize GPU rendering: {error}"));
                event_loop.exit();
                return;
            }
        };
        super::super::trace_host(&self.config, "gpu.ready");
        self.gpu_reconnect = None;
        #[cfg(target_os = "linux")]
        let imports_dmabuf = renderer.imports_dmabuf();
        #[cfg(windows)]
        let adapter = crate::osr::accel::adapter_luid(&renderer);
        self.renderer = Some(renderer);
        #[cfg(target_os = "linux")]
        if !imports_dmabuf {
            self.paint_in_software("the window's graphics device cannot import dma-bufs");
        }
        #[cfg(windows)]
        if self.browser_adapter.as_ref() != Some(&adapter) && self.browser_attached() {
            sabine_runtime::report_error(
                "gpu",
                "The browser paints on another graphics adapter; restarting it on the window's",
            );
            self.relaunch_browser();
        }
        self.launch_child();
        let restoring = !self.retained_frames.is_empty();
        if let Err(error) = self.restore_retained_frames() {
            self.fail(format!("Could not restore window textures: {error}"));
            return;
        }
        if restoring && self.main_surface.is_some() {
            self.present_rendered_surface("first_paint");
        } else {
            self.send_control("repaint\n");
        }
        if self.config.visible
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }
}
