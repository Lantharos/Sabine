use std::time::Instant;

use crate::osr::paint_rects::surface_rects;
use std::sync::Arc;

use crate::osr::protocol::{OsrPaintBatch, OsrSurface};
use crate::render::{GpuRenderer, ImageId, RendererError};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{
    OverlayLayer, PendingResizePaint, RESIZE_REPAINT_GRACE, RESIZE_REPAINT_RETRY, SurfaceGeometry,
};

impl OsrNativeHost {
    pub(in crate::osr::host) fn send_resize(&self) {
        let (width, height, scale) = self.content_size_for_cef();
        self.send_control(format!("resize\t{width}\t{height}\t{scale:.4}\n"));
    }

    pub(in crate::osr::host) fn queue_resize_paint(&mut self) {
        let size = self.content_surface_size();
        if self.main_surface_matches(size) {
            self.pending_resize_paint = None;
            return;
        }
        let now = Instant::now();
        self.pending_resize_paint = Some(PendingResizePaint {
            size,
            retry_at: now + RESIZE_REPAINT_RETRY,
            deadline: now + RESIZE_REPAINT_GRACE,
        });
        self.send_resize();
    }

    pub(in crate::osr::host) fn retry_resize_paint(&mut self) {
        let Some(mut pending) = self.pending_resize_paint else {
            return;
        };
        if self.main_surface_matches(pending.size) {
            self.pending_resize_paint = None;
            return;
        }
        let now = Instant::now();
        if now < pending.retry_at {
            return;
        }
        self.send_resize();
        pending.retry_at = now + RESIZE_REPAINT_RETRY;
        self.pending_resize_paint = Some(pending);
    }

    pub(in crate::osr::host) fn clear_pending_resize_paint(&mut self) {
        if self
            .pending_resize_paint
            .is_some_and(|pending| self.main_surface_matches(pending.size))
        {
            self.pending_resize_paint = None;
        }
    }

    pub(in crate::osr::host) fn main_surface_matches(&self, size: (u32, u32)) -> bool {
        self.main_surface
            .is_some_and(|surface| surface.size() == size)
    }

    pub(in crate::osr::host) fn main_surface_ready(&self) -> bool {
        self.main_load_ready && self.main_surface.is_some()
    }

    pub(in crate::osr::host) fn accepts_paint(&self) -> bool {
        (self.config.visible || self.config.lifecycle.retain_hidden_frame) && !self.occluded
    }

    pub(in crate::osr::host) fn surface_geometry(
        &self,
        pixel_size: (u32, u32),
        x: i32,
        y: i32,
    ) -> SurfaceGeometry {
        let (width, height) = self.frame_size_for_view(pixel_size);
        SurfaceGeometry {
            x,
            y,
            width,
            height,
        }
    }

    pub(in crate::osr::host) fn update_paint_batch(&mut self, batch: OsrPaintBatch) -> bool {
        if batch.rects.is_empty() {
            return false;
        }
        let pixel_size = (batch.width, batch.height);
        let geometry = self.surface_geometry(pixel_size, batch.x, batch.y);
        if batch.surface == OsrSurface::Main && geometry.size() != self.content_surface_size() {
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
            self.main_surface = Some(geometry);
            if self.main_load_ready {
                self.loading = None;
            }
            self.clear_pending_resize_paint();
        }
        true
    }

    pub(in crate::osr::host) fn clear_overlay(&mut self, overlay_id: &str) {
        let Some((id, _)) = self.overlays.remove_entry(overlay_id) else {
            return;
        };
        if let Some(renderer) = &mut self.renderer {
            renderer.remove_image(&ImageId::Overlay(id));
        }
    }

    /// The image a surface paints into, sharing the overlay's existing id.
    pub(in crate::osr::host) fn image_id(&self, surface: &OsrSurface) -> ImageId {
        let Some(overlay_id) = surface.overlay_id() else {
            return ImageId::Main;
        };
        ImageId::Overlay(
            self.overlays
                .get_key_value(overlay_id)
                .map_or_else(|| Arc::from(overlay_id), |(id, _)| Arc::clone(id)),
        )
    }

    pub(in crate::osr::host) fn place_overlay(
        &mut self,
        image: &ImageId,
        geometry: SurfaceGeometry,
    ) {
        if let ImageId::Overlay(id) = image {
            self.overlays
                .entry(Arc::clone(id))
                .or_insert_with(|| OverlayLayer::new(geometry))
                .geometry = geometry;
        }
    }

    /// Keeps the shown surfaces on the CPU before the renderer holding them
    /// goes away, so the window can show them again right away.
    pub(in crate::osr::host) fn retain_frames(&mut self) {
        let Some(renderer) = &self.renderer else {
            return;
        };
        let images = self
            .main_surface
            .map(|_| ImageId::Main)
            .into_iter()
            .chain(self.overlays.keys().cloned().map(ImageId::Overlay));
        self.retained_frames = images
            .filter_map(|id| Some((id.clone(), renderer.read_bgra_image(&id)?)))
            .collect();
        if !self.retained_frames.contains_key(&ImageId::Main) {
            self.main_surface = None;
        }
    }

    pub(in crate::osr::host) fn restore_retained_frames(&mut self) -> Result<(), RendererError> {
        let frames = std::mem::take(&mut self.retained_frames);
        let Some(renderer) = self.renderer.as_mut() else {
            return Ok(());
        };
        for (id, frame) in &frames {
            renderer.write_bgra_rects(id, frame.size(), &[frame.whole()])?;
        }
        Ok(())
    }
}

fn paint_surface(renderer: &mut GpuRenderer, image: &ImageId, batch: &OsrPaintBatch) -> bool {
    surface_rects(&batch.rects, batch.width, batch.height).is_some_and(|rects| {
        renderer
            .write_bgra_rects(image, (batch.width, batch.height), &rects)
            .is_ok()
    })
}
