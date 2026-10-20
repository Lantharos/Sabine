use winit::event_loop::ActiveEventLoop;

use super::OsrNativeHost;
use crate::render::{GpuRenderer, RendererError};

impl OsrNativeHost {
    /// Starts the window's graphics device off the event loop, so the
    /// browser's connection, bridge calls and frames keep flowing while the
    /// drivers start. When the compositor can show the page's frames itself,
    /// the device starts once the first one is on screen instead.
    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn start_rendering(&mut self, event_loop: &dyn ActiveEventLoop) {
        if !self.start_direct_presentation() {
            self.start_renderer(event_loop);
        }
    }

    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn start_renderer(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.renderer.is_some() || self.renderer_starting {
            return;
        }
        let Some(window) = self.window.clone() else {
            return;
        };
        self.renderer_starting = true;
        let generation = self.window_generation;
        let transparent = self.config.transparent;
        let sender = self.sender.clone();
        let proxy = self.proxy.clone();
        let started = std::thread::Builder::new()
            .name("sabine-gpu-start".to_string())
            .spawn(move || {
                let wake = proxy.clone();
                let renderer =
                    pollster::block_on(GpuRenderer::new(window, transparent, move || {
                        wake.wake_up()
                    }));
                let event = super::super::types::OsrHostEvent::RendererStarted(
                    generation,
                    Box::new(renderer),
                );
                if sender.send(event).is_ok() {
                    proxy.wake_up();
                }
            });
        if let Err(error) = started {
            self.fail(format!("Could not initialize GPU rendering: {error}"));
            event_loop.exit();
        }
    }

    pub(in crate::osr::host) fn renderer_started(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        renderer: Result<GpuRenderer, RendererError>,
    ) {
        #[cfg(target_os = "linux")]
        {
            self.renderer_starting = false;
        }
        let renderer = match renderer {
            Ok(renderer) => renderer,
            Err(error) => {
                self.fail(format!("Could not initialize GPU rendering: {error}"));
                event_loop.exit();
                return;
            }
        };
        super::super::trace_host(&self.config, "gpu.ready");
        #[cfg(target_os = "linux")]
        let imports_dmabuf = renderer.imports_dmabuf();
        self.renderer = Some(renderer);
        #[cfg(target_os = "linux")]
        if !imports_dmabuf {
            self.paint_in_software("the window's graphics device cannot import dma-bufs");
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

#[cfg(target_os = "linux")]
impl OsrNativeHost {
    fn start_direct_presentation(&mut self) -> bool {
        let fits = self.config.visible
            && !super::super::types::uses_sabine_chrome(self.config.chrome)
            && self.config.splash.is_none()
            && !self.software_paint
            && self.retained_frames.is_empty();
        let Some(window) = self.window.as_ref().filter(|_| fits) else {
            return false;
        };
        let sender = self.sender.clone();
        let proxy = self.proxy.clone();
        let generation = self.window_generation;
        self.direct_handover_ready = false;
        self.direct = super::direct::DirectPresenter::start(
            window.as_ref(),
            self.config.transparent,
            move |outcome| {
                let event =
                    super::super::types::OsrHostEvent::DirectPresentation(generation, outcome);
                if sender.send(event).is_ok() {
                    proxy.wake_up();
                }
            },
        );
        self.direct.is_some()
    }

    pub(in crate::osr::host) fn direct_presentation_changed(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        generation: u64,
        outcome: super::DirectOutcome,
    ) {
        if generation != self.window_generation {
            return;
        }
        match outcome {
            super::DirectOutcome::Shown => {
                super::super::trace_host(&self.config, "direct.shown");
                self.start_renderer(event_loop);
            }
            super::DirectOutcome::Failed(reason) => {
                sabine_runtime::report_error(
                    "render",
                    format!("the page's frames go through the window's graphics device: {reason}"),
                );
                self.direct = None;
                self.send_control("repaint\n");
                self.start_renderer(event_loop);
            }
            super::DirectOutcome::Drained => self.retiring_direct = None,
        }
    }

    /// Hands a page frame to the compositor while the window has no
    /// graphics device of its own yet.
    pub(in crate::osr::host) fn present_directly(
        &self,
        frame: &crate::osr::protocol::OsrAccelFrame,
        release: super::direct::Release,
    ) -> Result<(), super::direct::Release> {
        let (Some(direct), Some(dmabuf)) = (&self.direct, frame.resource.clone()) else {
            return Err(release);
        };
        if frame.surface != crate::osr::protocol::OsrSurface::Main {
            return Err(release);
        }
        direct.present(super::direct::DirectFrame {
            dmabuf,
            resource_id: frame.resource_id,
            format: frame.format,
            source: (
                frame.visible_x,
                frame.visible_y,
                frame.visible_width.min(self.surface_size.width),
                frame.visible_height.min(self.surface_size.height),
            ),
            destination: (
                self.logical_width().round() as i32,
                self.logical_height().round() as i32,
            ),
            release,
        });
        Ok(())
    }

    /// Puts the window on screen with the directly shown page, over what the
    /// window's device would have drawn beneath it.
    pub(in crate::osr::host) fn map_direct_presentation(&self) {
        let Some(direct) = &self.direct else {
            return;
        };
        let color = self.config.background_color.to_rgba8();
        let backdrop = if self.config.transparent {
            let scale = self.scale();
            let physical = |logical: i32| (f64::from(logical.max(0)) * scale).round() as u32;
            let rects = self
                .regions()
                .opaque
                .map(|opaque| {
                    opaque.resolved_rects(
                        self.logical_width().round().max(1.0) as i32,
                        self.logical_height().round().max(1.0) as i32,
                    )
                })
                .unwrap_or_default()
                .into_iter()
                .map(|rect| {
                    let (left, top) = (physical(rect.x), physical(rect.y));
                    let right = physical(rect.x.saturating_add(rect.width));
                    let bottom = physical(rect.y.saturating_add(rect.height));
                    (
                        left,
                        top,
                        right.saturating_sub(left),
                        bottom.saturating_sub(top),
                    )
                })
                .collect();
            super::direct::Backdrop {
                width: self.surface_size.width,
                height: self.surface_size.height,
                color,
                transparent: true,
                rects,
            }
        } else {
            super::direct::Backdrop {
                width: 1,
                height: 1,
                color,
                transparent: false,
                rects: vec![(0, 0, 1, 1)],
            }
        };
        direct.map(backdrop);
    }

    /// Notes that the window's own device holds the page, so it can replace
    /// the directly shown frame.
    pub(in crate::osr::host) fn note_main_frame_drawable(&mut self) {
        self.direct_handover_ready = self.direct.is_some();
    }

    /// Takes the directly shown page down with the window's first own frame,
    /// once the window's device holds a page frame. Returns whether the
    /// window can draw now.
    pub(in crate::osr::host) fn hand_over_direct_presentation(&mut self) -> bool {
        let Some(direct) = &self.direct else {
            return true;
        };
        if !self.direct_handover_ready {
            return false;
        }
        direct.hide_with_next_commit();
        self.retiring_direct = self.direct.take();
        true
    }
}
