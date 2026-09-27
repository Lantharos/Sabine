use crate::osr::protocol::{MAIN_TEXTURE_ID, OsrAccelFrame, OsrSurface};

use super::native::OsrNativeHost;
use super::types::{OverlayLayer, overlay_texture_id};

impl OsrNativeHost {
    pub(super) fn update_accel_frame(&mut self, frame: OsrAccelFrame) -> bool {
        if !self.try_install_accel_texture(&frame) {
            return false;
        }
        self.note_accel_surface(&frame);
        true
    }

    fn try_install_accel_texture(&mut self, frame: &OsrAccelFrame) -> bool {
        let release_writer = self.control_writer.clone();
        let slot_token = frame.slot_token;
        let release_slot = move || {
            if let Some(writer) = release_writer {
                let _ = writer.send(format!("accel_release\t{slot_token}\n"));
            }
        };
        let geometry = self.accel_geometry(frame);
        if frame.surface == OsrSurface::Main && geometry.size() != self.content_surface_size() {
            release_slot();
            self.retry_resize_paint();
            return false;
        }
        let Some(renderer) = self.renderer.as_mut() else {
            release_slot();
            return false;
        };
        let texture_id = frame
            .surface
            .overlay_id()
            .map_or_else(|| MAIN_TEXTURE_ID.to_string(), overlay_texture_id);
        match crate::osr::accel::try_import_d3d12(renderer, frame) {
            Ok(texture) => crate::osr::accel::install_imported_texture(
                renderer,
                &texture_id,
                frame,
                texture,
                release_slot,
            )
            .is_ok(),
            Err(error) => {
                eprintln!("Sabine OSR: accelerated texture import failed: {error}");
                release_slot();
                false
            }
        }
    }

    fn note_accel_surface(&mut self, frame: &OsrAccelFrame) {
        let geometry = self.accel_geometry(frame);
        match frame.surface.overlay_id() {
            None => {
                self.main_buffer.release();
                self.main_surface = Some(geometry);
                if self.main_load_ready {
                    self.loading = None;
                }
                self.clear_pending_resize_paint();
            }
            Some(overlay_id) => {
                let overlay = self
                    .overlays
                    .entry(overlay_id.to_string())
                    .or_insert_with(|| OverlayLayer::new(geometry));
                overlay.buffer.release();
                overlay.geometry = geometry;
            }
        }
    }

    fn accel_geometry(&self, frame: &OsrAccelFrame) -> super::types::SurfaceGeometry {
        self.surface_geometry(
            (frame.visible_width, frame.visible_height),
            frame.x,
            frame.y,
        )
    }
}
