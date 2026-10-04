use winit::dpi::PhysicalSize;

use crate::osr::host::native::OsrNativeHost;

impl OsrNativeHost {
    pub(super) fn surface_resized(&mut self, size: PhysicalSize<u32>) {
        self.sync_active_frame_rate();
        let minimized = size.width == 0 || size.height == 0;
        #[cfg(windows)]
        self.set_occluded(minimized);
        if minimized {
            return;
        }
        let Some(scale) = self.window.as_ref().map(|window| window.scale_factor()) else {
            return;
        };
        if self.is_redundant_configure(size, scale) {
            return;
        }
        self.apply_surface_geometry(size, scale);
    }

    pub(super) fn scale_factor_changed(&mut self, scale: f64) {
        self.sync_active_frame_rate();
        let Some(size) = self.window.as_ref().map(|window| window.surface_size()) else {
            return;
        };
        self.apply_surface_geometry(size, scale);
    }

    /// Wayland repeats the current configure after an interactive move;
    /// reconfiguring the swapchain or Chromium for it flashes the window.
    fn is_redundant_configure(&self, size: PhysicalSize<u32>, scale: f64) -> bool {
        size == self.surface_size && (scale - self.scale_factor).abs() < f64::EPSILON
    }

    fn apply_surface_geometry(&mut self, size: PhysicalSize<u32>, scale: f64) {
        self.cancel_context_menu();
        self.surface_size = size;
        self.scale_factor = scale;
        self.effect_regions_dirty = true;
        self.relayout_media();
        #[cfg(target_os = "macos")]
        if let (Some(renderer), Some(window)) = (self.renderer.as_mut(), self.window.as_ref()) {
            renderer.set_fullscreen(window.fullscreen().is_some());
        }
        self.queue_resize_paint();
        if self.presented {
            self.render();
        } else if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(super) fn focus_changed(&mut self, focused: bool) {
        self.sync_active_frame_rate();
        let focused = focused && self.config.visible;
        self.focused = focused;
        if focused {
            self.refresh_appearance();
        } else {
            self.cancel_context_menu();
        }
        self.send_control(if focused { "focus\t1\n" } else { "focus\t0\n" });
        if !focused && self.config.hide_on_blur && self.config.visible {
            self.hide_window("blur");
        } else {
            self.schedule_lifecycle_sync(if focused { "focus" } else { "blur" });
        }
    }

    /// Windows reports a minimized window only through its zero-sized surface,
    /// so minimizing there counts as the window being out of sight, as Wayland
    /// reports it.
    pub(super) fn set_occluded(&mut self, occluded: bool) {
        if self.occluded == occluded {
            return;
        }
        self.occluded = occluded;
        self.media.set_occluded(occluded);
        self.schedule_lifecycle_sync(if occluded { "occluded" } else { "visible" });
    }
}
