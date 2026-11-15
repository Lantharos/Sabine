use winit::event_loop::ActiveEventLoop;

use super::{Backdrop, DirectFrame, DirectOutcome, DirectPresenter};
use crate::osr::control::SlotRelease;
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
            DirectOutcome::Hidden(transition) => {
                if self.settle_direct_presentation(transition) {
                    self.draw_now();
                }
            }
            DirectOutcome::Revealed(transition) => {
                if self.settle_direct_presentation(transition) {
                    self.page_revealed();
                }
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
                self.draw_now();
            }
        }
    }

    fn settle_direct_presentation(&mut self, transition: u64) -> bool {
        self.direct
            .as_mut()
            .is_some_and(|direct| direct.settle(transition))
    }

    /// Draws the window's next frame now, or once it can.
    fn draw_now(&mut self) {
        if !self.render()
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }

    /// Lets go of the page frame the window's device drew, now that the
    /// subsurface shows a newer one from the window's next commit.
    fn page_revealed(&mut self) {
        self.direct_handover_ready = false;
        self.direct_repaint_requested = false;
        if let Some(renderer) = &mut self.renderer {
            renderer.remove_image(&ImageId::Main);
        }
        self.draw_now();
        if let Some(direct) = &self.direct {
            direct.desync();
        }
    }

    /// Whether the window draws nothing over or around the page but its
    /// backdrop, so the page can stay in its subsurface.
    fn page_stays_direct(&self) -> bool {
        self.overlays.is_empty()
            && self.tooltip.as_ref().is_none_or(|tooltip| !tooltip.shown)
            && self.context_menu.is_none()
            && self.permission_prompt.is_none()
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
            return if direct.shows_page() && !self.holds_last_frame() {
                PageRoute::Subsurface
            } else {
                PageRoute::Window
            };
        }
        match (self.page_stays_direct(), direct.shows_page()) {
            (false, _) => PageRoute::Window,
            (true, true) => PageRoute::Subsurface,
            (true, false) => PageRoute::Reveal,
        }
    }

    /// Whether the subsurface shows the page, so the window's device leaves
    /// it out.
    pub(in crate::osr::host) fn page_in_subsurface(&self) -> bool {
        self.direct
            .as_ref()
            .is_some_and(DirectPresenter::shows_page)
    }

    fn direct_frame(&self, frame: &OsrAccelFrame, release: SlotRelease) -> Option<DirectFrame> {
        Some(DirectFrame {
            dmabuf: frame.resource.clone()?,
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
            damage: frame.damage,
            sequence: frame.sequence,
            release,
            inputs: self.painted_inputs(),
        })
    }

    /// Hands a page frame to the compositor. Returns whether it took it.
    pub(in crate::osr::host) fn present_directly(
        &mut self,
        frame: &OsrAccelFrame,
        release: SlotRelease,
    ) -> bool {
        let Some(frame) = self.direct_frame(frame, release) else {
            return false;
        };
        if !self
            .direct
            .as_ref()
            .is_some_and(|direct| direct.present(frame))
        {
            return false;
        }
        self.forget_window_page();
        true
    }

    /// Moves the page from the window's device back into the subsurface,
    /// with the window frame that follows the compositor taking the frame.
    /// Returns whether the compositor was handed the frame.
    pub(in crate::osr::host) fn reveal_directly(
        &mut self,
        frame: &OsrAccelFrame,
        release: SlotRelease,
    ) -> bool {
        let Some(frame) = self.direct_frame(frame, release) else {
            return false;
        };
        self.direct
            .as_mut()
            .is_some_and(|direct| direct.reveal_with_next_commit(frame))
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
        let [red, green, blue, alpha] = self.config.background_color.to_rgba8();
        let backdrop = if self.config.transparent {
            Backdrop {
                color: [red, green, blue, alpha],
                transparent: true,
                rects: self
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
                    .map(|rect| (rect.x, rect.y, rect.width, rect.height))
                    .collect(),
            }
        } else {
            Backdrop {
                color: [red, green, blue, u8::MAX],
                transparent: false,
                rects: Vec::new(),
            }
        };
        direct.map(backdrop);
    }

    /// Notes that the window's own device holds the newest page frame, so
    /// it can replace the one the subsurface shows or is about to.
    pub(in crate::osr::host) fn note_main_frame_drawable(&mut self) {
        self.direct_handover_ready = self
            .direct
            .as_ref()
            .is_some_and(DirectPresenter::holds_page);
    }

    /// Takes the page out of the subsurface once the window draws something
    /// with it and its device holds a page frame, asking the browser for
    /// one first. The window draws once the subsurface took the page down.
    /// Returns whether the window can draw now.
    pub(in crate::osr::host) fn hand_over_direct_presentation(&mut self) -> bool {
        let stays = self.page_stays_direct();
        let drawing = self.renderer.is_some();
        let Some(direct) = self.direct.as_mut() else {
            return true;
        };
        if drawing {
            direct.retire_backdrop();
        }
        if direct.hiding() {
            return false;
        }
        if !direct.holds_page() || (stays && !direct.failed()) {
            return true;
        }
        if std::mem::take(&mut self.direct_handover_ready) {
            direct.hide_with_next_commit();
            self.direct_repaint_requested = false;
            return !direct.hiding();
        }
        if !std::mem::replace(&mut self.direct_repaint_requested, true) {
            self.send_control("repaint\n");
        }
        false
    }
}
