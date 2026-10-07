use winit::dpi::PhysicalPosition;

use super::OsrNativeHost;
use crate::osr::host::types::uses_sabine_chrome;
use crate::render::effective_scale;

impl OsrNativeHost {
    pub(in crate::osr::host) fn scale(&self) -> f64 {
        effective_scale(
            self.window
                .as_ref()
                .map_or(self.scale_factor, |window| window.scale_factor()),
        )
    }

    pub(in crate::osr::host) fn logical_point(
        &self,
        position: PhysicalPosition<f64>,
    ) -> (f32, f32) {
        let scale = self.scale();
        ((position.x / scale) as f32, (position.y / scale) as f32)
    }

    pub(in crate::osr::host) fn logical_width(&self) -> f32 {
        (f64::from(self.surface_size.width) / self.scale()) as f32
    }

    pub(in crate::osr::host) fn logical_height(&self) -> f32 {
        (f64::from(self.surface_size.height) / self.scale()) as f32
    }

    pub(in crate::osr::host) fn titlebar_height(&self) -> f32 {
        if uses_sabine_chrome(self.config.chrome) {
            crate::osr::host::types::TITLEBAR_HEIGHT
        } else {
            0.0
        }
    }

    pub(in crate::osr::host) fn content_size_for_cef(&self) -> (u32, u32, f64) {
        let scale = self.scale();
        if !self.config.visible
            && self.window.is_none()
            && !self.config.lifecycle.retain_hidden_frame
        {
            return (1, 1, scale);
        }
        let logical_width = f64::from(self.surface_size.width) / scale;
        let logical_height = (f64::from(self.surface_size.height) / scale
            - f64::from(self.titlebar_height()))
        .max(1.0);
        (
            logical_width.round().max(1.0) as u32,
            logical_height.round().max(1.0) as u32,
            scale,
        )
    }

    pub(in crate::osr::host) fn content_surface_size(&self) -> (u32, u32) {
        let (width, height, _) = self.content_size_for_cef();
        (width, height)
    }

    pub(in crate::osr::host) fn frame_size_for_view(&self, size: (u32, u32)) -> (u32, u32) {
        let scale = self.scale();
        (
            (f64::from(size.0) / scale).round().max(1.0) as u32,
            (f64::from(size.1) / scale).round().max(1.0) as u32,
        )
    }

    /// The page position under a window point, or `None` over the titlebar.
    pub(in crate::osr::host) fn content_position(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let titlebar_height = self.titlebar_height();
        (y >= titlebar_height).then(|| self.clamped_content_position(x, y))
    }

    /// The page position nearest a window point, for pointers that must reach
    /// the page wherever they end, such as a lifted finger.
    pub(in crate::osr::host) fn clamped_content_position(&self, x: f32, y: f32) -> (f32, f32) {
        (x.max(0.0), (y - self.titlebar_height()).max(0.0))
    }
}
