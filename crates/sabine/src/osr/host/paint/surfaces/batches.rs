use crate::osr::paint_rects::surface_rects;
use crate::osr::protocol::{OsrPaintBatch, OsrSurface};
use crate::render::{GpuRenderer, ImageId};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::SurfaceGeometry;

impl OsrNativeHost {
    pub(in crate::osr::host) fn main_surface_ready(&self) -> bool {
        self.page_ready() && self.main_surface.is_some()
    }

    pub(in crate::osr::host) fn accepts_paint(&self) -> bool {
        (self.config.visible || self.config.lifecycle.retain_hidden_frame) && !self.occluded
    }

    pub(in crate::osr::host) fn update_paint_batch(&mut self, batch: OsrPaintBatch) -> bool {
        if batch.rects.is_empty() {
            return false;
        }
        let geometry = SurfaceGeometry {
            x: batch.x,
            y: batch.y,
            width: batch.width,
            height: batch.height,
        };
        if batch.surface == OsrSurface::Main && !self.fits_page(geometry) {
            self.note_rejected_main_frame(geometry.size());
            self.retry_resize_paint();
            return false;
        }
        let image = self.image_id(&batch.surface);
        let Some(renderer) = self.renderer.as_mut() else {
            return false;
        };
        if !paint_surface(renderer, &image, &batch) {
            return false;
        }
        self.place_overlay(&image, geometry);
        if batch.surface == OsrSurface::Main {
            #[cfg(target_os = "linux")]
            self.note_main_frame_drawable();
            self.main_surface = Some(geometry);
            if self.page_ready() {
                self.finish_loading();
            }
            self.clear_pending_resize_paint();
        }
        true
    }
}

fn paint_surface(renderer: &mut GpuRenderer, image: &ImageId, batch: &OsrPaintBatch) -> bool {
    surface_rects(&batch.rects, batch.width, batch.height).is_some_and(|rects| {
        renderer
            .write_bgra_rects(image, (batch.width, batch.height), &rects)
            .is_ok()
    })
}
