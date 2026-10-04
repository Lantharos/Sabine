use std::time::Instant;

use crate::osr::paint_rects::surface_rects;
use crate::osr::protocol::{MAIN_TEXTURE_ID, OsrPaintBatch, OsrSurface};
use crate::render::{GpuRenderer, RendererError};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{
    LifecycleState, OverlayLayer, PendingResizePaint, RESIZE_REPAINT_GRACE, RESIZE_REPAINT_RETRY,
    SurfaceGeometry, overlay_texture_id,
};

impl OsrNativeHost {
    pub(in crate::osr::host) fn send_resize(&self) {
        let (width, height, scale) = self.content_size_for_cef();
        self.send_control(format!("resize\t{width}\t{height}\t{scale:.4}\n"));
    }

    pub(in crate::osr::host) fn queue_resize_paint(&mut self) {
        let size = self.content_surface_size();
        if self.main_surface_matches(size) {
            // Content size unchanged — do not poke CEF (WasResized/Invalidate
            // blanks OSR until the next paint, which shows as a drag-end flash).
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
        // Keep compositing while FPS-throttled (blur/occlusion suspend). Only
        // stop accepting paints when the view is actually gone.
        (self.config.visible || self.config.lifecycle.retain_hidden_frame)
            && !self.occluded
            && !matches!(
                self.lifecycle_state,
                LifecycleState::Hibernating | LifecycleState::Hibernated
            )
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
        let Some(renderer) = self.renderer.as_mut() else {
            return false;
        };
        let uploaded = match batch.surface.overlay_id() {
            None => paint_surface(renderer, MAIN_TEXTURE_ID, &batch),
            Some(overlay_id) => {
                self.overlays
                    .entry(overlay_id.to_string())
                    .or_insert_with(|| OverlayLayer::new(geometry))
                    .geometry = geometry;
                paint_surface(renderer, &overlay_texture_id(overlay_id), &batch)
            }
        };
        if !uploaded {
            return false;
        }
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
        self.overlays.remove(overlay_id);
        if let Some(renderer) = &mut self.renderer {
            renderer.remove_image(&overlay_texture_id(overlay_id));
        }
    }

    /// Keeps the shown surfaces on the CPU before the renderer holding them
    /// goes away, so the window can show them again right away.
    pub(in crate::osr::host) fn retain_frames(&mut self) {
        let Some(renderer) = &self.renderer else {
            return;
        };
        let texture_ids = self
            .main_surface
            .map(|_| MAIN_TEXTURE_ID.to_string())
            .into_iter()
            .chain(self.overlays.keys().map(|id| overlay_texture_id(id)));
        self.retained_frames = texture_ids
            .filter_map(|id| Some((id.clone(), renderer.read_bgra_image(&id)?)))
            .collect();
        if !self.retained_frames.contains_key(MAIN_TEXTURE_ID) {
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

fn paint_surface(renderer: &mut GpuRenderer, texture_id: &str, batch: &OsrPaintBatch) -> bool {
    surface_rects(&batch.rects, batch.width, batch.height).is_some_and(|rects| {
        renderer
            .write_bgra_rects(texture_id, (batch.width, batch.height), &rects)
            .is_ok()
    })
}
