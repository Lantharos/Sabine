use crate::osr::protocol::{MAIN_TEXTURE_ID, OsrAccelFrame, OsrSurface};
use crate::render::ExternalSlot;

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{OverlayLayer, overlay_texture_id};

impl OsrNativeHost {
    pub(in crate::osr::host) fn update_accel_frame(&mut self, frame: OsrAccelFrame) -> bool {
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
        let slot = ExternalSlot {
            index: frame.resource_slot,
            resource_id: frame.resource_id,
        };
        let installed = renderer.set_external_bgra_texture(
            &texture_id,
            slot,
            |device| crate::osr::accel::import_texture(device, frame),
            (frame.visible_x, frame.visible_y),
            (frame.visible_width, frame.visible_height),
            release_slot,
        );
        if let Err(error) = &installed {
            eprintln!("Sabine OSR: accelerated texture import failed: {error}");
            #[cfg(target_os = "linux")]
            if matches!(error, crate::render::RendererError::Texture(_)) {
                self.paint_in_software("the window could not import the browser's frames");
            }
        }
        installed.is_ok()
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

    fn accel_geometry(&self, frame: &OsrAccelFrame) -> crate::osr::host::types::SurfaceGeometry {
        self.surface_geometry(
            (frame.visible_width, frame.visible_height),
            frame.x,
            frame.y,
        )
    }
}
