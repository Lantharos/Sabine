use winit::event_loop::ActiveEventLoop;

use super::{Backdrop, DirectFrame, DirectOutcome, DirectPresenter, Release};
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{OsrHostEvent, uses_sabine_chrome};
use crate::osr::protocol::OsrAccelFrame;
use crate::render::ImageId;

/// Where the page's next frame goes.
pub(in crate::osr::host) enum PageRoute {
    /// Into the subsurface, which shows the page now.
    Subsurface,
    /// Into the subsurface, taking the page back from the window's device.
    Reveal,
    /// Into the window's device, which draws the page with the rest.
    Window,
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn start_direct_presentation(&mut self) -> bool {
        let fits = self.config.visible
            && !uses_sabine_chrome(self.config.chrome)
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
        self.direct = DirectPresenter::start(
            window.as_ref(),
            self.config.transparent,
            self.input_latency.clone(),
            move |outcome| {
                let event = OsrHostEvent::DirectPresentation(generation, outcome);
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
        outcome: DirectOutcome,
    ) {
        if generation != self.window_generation {
            return;
        }
        match outcome {
            DirectOutcome::Shown => {
                crate::osr::host::trace_host(&self.config, "direct.shown");
                self.start_renderer(event_loop, false);
            }
            DirectOutcome::Failed(reason) => {
                sabine_runtime::report_error(
                    "render",
                    format!("the page's frames go through the window's graphics device: {reason}"),
                );
                match (&mut self.direct, &self.renderer) {
                    (Some(direct), Some(_)) => direct.note_failure(),
                    _ => self.direct = None,
                }
                self.send_control("repaint\n");
                self.start_renderer(event_loop, false);
            }
        }
    }

    /// Whether the window draws nothing over or around the page but its
    /// backdrop, so the page can stay in its subsurface.
    fn page_stays_direct(&self) -> bool {
        self.overlays.is_empty()
            && self.tooltip.as_ref().is_none_or(|tooltip| !tooltip.shown)
            && self.context_menu.is_none()
            && self.media.holes().next().is_none()
            && self.loading.is_none_or(|loading| !loading.splash_shown())
            && self.loading_fade.is_none()
            && !self.holds_last_frame()
            && self.pending_resize_paint.is_none()
    }

    pub(in crate::osr::host) fn page_route(&self) -> PageRoute {
        let Some(direct) = self.direct.as_ref().filter(|direct| !direct.failed()) else {
            return PageRoute::Window;
        };
        if self.renderer.is_none() {
            return if direct.hidden() || self.holds_last_frame() {
                PageRoute::Window
            } else {
                PageRoute::Subsurface
            };
        }
        match (self.page_stays_direct(), direct.hidden()) {
            (false, _) => PageRoute::Window,
            (true, false) => PageRoute::Subsurface,
            (true, true) => PageRoute::Reveal,
        }
    }

    /// Whether the subsurface shows the page, so the window's device leaves
    /// it out.
    pub(in crate::osr::host) fn page_in_subsurface(&self) -> bool {
        self.direct.as_ref().is_some_and(|direct| !direct.hidden())
    }

    fn direct_frame(&self, frame: &OsrAccelFrame, release: Release) -> Option<DirectFrame> {
        let Some(dmabuf) = frame.resource.clone() else {
            release();
            return None;
        };
        Some(DirectFrame {
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
            inputs: self.painted_inputs(),
        })
    }

    /// Hands a page frame to the compositor. Returns whether it took it.
    pub(in crate::osr::host) fn present_directly(
        &mut self,
        frame: &OsrAccelFrame,
        release: Release,
    ) -> bool {
        let Some(frame) = self.direct_frame(frame, release) else {
            return false;
        };
        let Some(direct) = &self.direct else {
            (frame.release)();
            return false;
        };
        direct.present(frame);
        self.forget_window_page();
        true
    }

    /// Moves the page from the window's device back into the subsurface,
    /// with a window frame that leaves it out. Hands the slot's release back
    /// while the compositor still imports the frame's dma-buf.
    pub(in crate::osr::host) fn reveal_directly(
        &mut self,
        frame: &OsrAccelFrame,
        release: Release,
    ) -> Result<(), Release> {
        let Some(direct_frame) = self.direct_frame(frame, release) else {
            return Ok(());
        };
        let Some(direct) = &mut self.direct else {
            return Err(direct_frame.release);
        };
        direct
            .reveal_with_next_commit(direct_frame)
            .map_err(|frame| frame.release)?;
        self.forget_window_page();
        if !self.render()
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
        if let Some(direct) = &self.direct {
            direct.desync();
        }
        Ok(())
    }

    /// Lets go of the page frame the window's device held, now that the
    /// subsurface shows a newer one.
    fn forget_window_page(&mut self) {
        self.direct_repaint_requested = false;
        if std::mem::take(&mut self.direct_handover_ready)
            && let Some(renderer) = &mut self.renderer
        {
            renderer.remove_image(&ImageId::Main);
        }
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
            Backdrop {
                width: self.surface_size.width,
                height: self.surface_size.height,
                color,
                transparent: true,
                rects,
            }
        } else {
            Backdrop {
                width: 1,
                height: 1,
                color,
                transparent: false,
                rects: vec![(0, 0, 1, 1)],
            }
        };
        direct.map(backdrop);
    }

    /// Notes that the window's own device holds the newest page frame, so
    /// it can replace the directly shown one.
    pub(in crate::osr::host) fn note_main_frame_drawable(&mut self) {
        self.direct_handover_ready = self.page_in_subsurface();
    }

    /// Takes the page out of the subsurface with the window's next frame
    /// once the window draws something with it and its device holds a page
    /// frame, asking the browser for one first. Returns whether the window
    /// can draw now.
    pub(in crate::osr::host) fn hand_over_direct_presentation(&mut self) -> bool {
        let stays = self.page_stays_direct();
        let Some(direct) = self.direct.as_mut().filter(|direct| !direct.hidden()) else {
            return true;
        };
        if self.direct_handover_ready {
            direct.hide_with_next_commit();
            self.direct_handover_ready = false;
            self.direct_repaint_requested = false;
            return true;
        }
        if stays && !direct.failed() {
            return true;
        }
        if !std::mem::replace(&mut self.direct_repaint_requested, true) {
            self.send_control("repaint\n");
        }
        false
    }
}
