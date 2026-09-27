use std::time::Instant;

use crate::osr::frame_buffer::FrameBuffer;
use crate::osr::protocol::{MAIN_TEXTURE_ID, OsrPaintBatch, OsrSurface};
use crate::render::{GpuRenderer, PixelRect, RendererError};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{
    LifecycleState, OverlayLayer, PendingResizePaint, RESIZE_REPAINT_GRACE, RESIZE_REPAINT_RETRY,
    SurfaceGeometry, overlay_texture_id,
};

impl OsrNativeHost {
    pub(in crate::osr::host) fn send_resize(&self) {
        let (width, height, scale) = self.content_size_for_cef();
        self.send_control(&format!("resize\t{width}\t{height}\t{scale:.4}\n"));
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

    pub(in crate::osr::host) fn frame_size_for_view(&self, size: (u32, u32)) -> (u32, u32) {
        let scale = self
            .window
            .as_ref()
            .map_or(self.scale_factor, |window| window.scale_factor())
            .max(1.0);
        (
            (f64::from(size.0) / scale).round().max(1.0) as u32,
            (f64::from(size.1) / scale).round().max(1.0) as u32,
        )
    }

    pub(in crate::osr::host) fn accepts_paint(&self) -> bool {
        // Keep compositing while FPS-throttled (blur/occlusion suspend). Only
        // stop accepting paints when the view is actually gone.
        (self.config.visible || self.config.lifecycle.retain_hidden_frame)
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
            None => paint_surface(renderer, MAIN_TEXTURE_ID, &mut self.main_buffer, &batch),
            Some(overlay_id) => {
                let overlay = self
                    .overlays
                    .entry(overlay_id.to_string())
                    .or_insert_with(|| OverlayLayer::new(geometry));
                overlay.geometry = geometry;
                paint_surface(
                    renderer,
                    &overlay_texture_id(overlay_id),
                    &mut overlay.buffer,
                    &batch,
                )
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

    pub(in crate::osr::host) fn upload_cached_textures(&mut self) -> Result<(), RendererError> {
        let Some(renderer) = self.renderer.as_mut() else {
            return Ok(());
        };
        if self.main_surface.is_some() {
            upload_entire(renderer, MAIN_TEXTURE_ID, &self.main_buffer)?;
        }
        for (id, overlay) in &self.overlays {
            upload_entire(renderer, &overlay_texture_id(id), &overlay.buffer)?;
        }
        Ok(())
    }
}

fn paint_surface(
    renderer: &mut GpuRenderer,
    texture_id: &str,
    buffer: &mut FrameBuffer,
    batch: &OsrPaintBatch,
) -> bool {
    let Some(damage) = buffer.compose(batch.width, batch.height, &batch.rects) else {
        return false;
    };
    let bounds = damage
        .iter()
        .copied()
        .reduce(PixelRect::union)
        .expect("composed paint has damage");
    let damaged_area = damage.iter().map(|rect| rect.area()).sum::<u64>();
    let regions = if bounds.area() > damaged_area.saturating_mul(2) {
        damage.as_slice()
    } else {
        std::slice::from_ref(&bounds)
    };
    renderer
        .upload_bgra_regions(texture_id, buffer.size(), buffer.bytes(), regions)
        .is_ok()
}

fn upload_entire(
    renderer: &mut GpuRenderer,
    texture_id: &str,
    buffer: &FrameBuffer,
) -> Result<(), RendererError> {
    if buffer.bytes().is_empty() {
        return Ok(());
    }
    let (width, height) = buffer.size();
    renderer.upload_bgra_regions(
        texture_id,
        (width, height),
        buffer.bytes(),
        &[PixelRect {
            x: 0,
            y: 0,
            width,
            height,
        }],
    )
}
