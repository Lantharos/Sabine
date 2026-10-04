use crate::osr::protocol::{OsrAccelFrame, OsrSurface};
use crate::render::{ExternalSlot, ImageId};

use crate::osr::host::native::OsrNativeHost;

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
        let image = self.image_id(&frame.surface);
        let Some(renderer) = self.renderer.as_mut() else {
            release_slot();
            return false;
        };
        let slot = ExternalSlot {
            index: frame.resource_slot,
            resource_id: frame.resource_id,
        };
        let installed = renderer.set_external_bgra_texture(
            &image,
            slot,
            |device| crate::osr::accel::import_texture(device, frame),
            (frame.visible_x, frame.visible_y),
            (frame.visible_width, frame.visible_height),
            release_slot,
        );
        if let Err(error) = &installed {
            sabine_runtime::report_error(
                "render",
                format!("could not show a browser frame: {error}"),
            );
        }
        installed.is_ok()
    }

    fn note_accel_surface(&mut self, frame: &OsrAccelFrame) {
        let geometry = self.accel_geometry(frame);
        let image = self.image_id(&frame.surface);
        if image == ImageId::Main {
            self.main_surface = Some(geometry);
            if self.main_load_ready {
                self.loading = None;
            }
            self.clear_pending_resize_paint();
        } else {
            self.place_overlay(&image, geometry);
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
