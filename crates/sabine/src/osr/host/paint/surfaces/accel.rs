use crate::osr::protocol::{OsrAccelFrame, OsrSurface};
use crate::render::{ExternalSlot, ImageId, RendererError};

use crate::osr::host::native::OsrNativeHost;
#[cfg(target_os = "linux")]
use crate::osr::host::native::PageRoute;
use crate::osr::host::types::SurfaceGeometry;

/// What became of an accelerated frame.
enum Installed {
    /// The window's device holds it, to draw with the window's next frame.
    Window,
    /// The compositor shows it straight away.
    #[cfg(target_os = "linux")]
    Subsurface,
    Rejected,
}

impl OsrNativeHost {
    /// Takes in an accelerated frame. Returns whether the window has to
    /// draw a frame to show it.
    pub(in crate::osr::host) fn update_accel_frame(&mut self, frame: OsrAccelFrame) -> bool {
        let installed = self.install_accel_frame(&frame);
        if matches!(installed, Installed::Rejected) {
            return false;
        }
        self.note_accel_surface(&frame);
        matches!(installed, Installed::Window)
    }

    fn install_accel_frame(&mut self, frame: &OsrAccelFrame) -> Installed {
        let release_writer = self.control_writer.clone();
        let slot_token = frame.slot_token;
        let release_slot: Box<dyn FnOnce() + Send> = Box::new(move || {
            if let Some(writer) = release_writer {
                let _ = writer.send(format!("accel_release\t{slot_token}\n"));
            }
        });
        let geometry = accel_geometry(frame);
        if frame.surface == OsrSurface::Main && !self.fits_page(geometry) {
            self.note_rejected_main_frame(geometry.size());
            release_slot();
            self.retry_resize_paint();
            return Installed::Rejected;
        }
        let image = self.image_id(&frame.surface);
        #[cfg(target_os = "linux")]
        let release_slot = if image == ImageId::Main {
            match self.page_route() {
                PageRoute::Subsurface => {
                    return if self.present_directly(frame, release_slot) {
                        Installed::Subsurface
                    } else {
                        Installed::Rejected
                    };
                }
                PageRoute::Reveal => match self.reveal_directly(frame, release_slot) {
                    Ok(()) => return Installed::Subsurface,
                    Err(release_slot) => release_slot,
                },
                PageRoute::Window => release_slot,
            }
        } else {
            release_slot
        };
        if self.install_accel_texture(frame, &image, release_slot) {
            Installed::Window
        } else {
            Installed::Rejected
        }
    }

    fn install_accel_texture(
        &mut self,
        frame: &OsrAccelFrame,
        image: &ImageId,
        release_slot: Box<dyn FnOnce() + Send>,
    ) -> bool {
        let Some(renderer) = self.renderer.as_mut() else {
            release_slot();
            return false;
        };
        let slot = ExternalSlot {
            index: frame.resource_slot,
            resource_id: frame.resource_id,
        };
        let installed = renderer.set_external_bgra_texture(
            image,
            slot,
            |device| crate::osr::accel::import_texture(device, frame),
            (frame.visible_x, frame.visible_y),
            (frame.visible_width, frame.visible_height),
            release_slot,
        );
        #[cfg(target_os = "linux")]
        if let (Ok(()), Some(dmabuf)) = (&installed, &frame.resource) {
            use std::os::fd::AsFd;
            renderer.wait_for_dmabuf(dmabuf.fd.as_fd());
        }
        if let Err(error) = &installed {
            let import_unsupported =
                matches!(error, RendererError::Texture(_)) && !renderer.lost_device();
            sabine_runtime::report_error(
                "render",
                format!("could not show a browser frame: {error}"),
            );
            if import_unsupported {
                self.paint_in_software("the window could not import the browser's frames");
            }
        }
        #[cfg(target_os = "linux")]
        if installed.is_ok() && *image == ImageId::Main {
            self.note_main_frame_drawable();
        }
        installed.is_ok()
    }

    fn note_accel_surface(&mut self, frame: &OsrAccelFrame) {
        let geometry = accel_geometry(frame);
        let image = self.image_id(&frame.surface);
        if image == ImageId::Main {
            self.main_surface = Some(geometry);
            if self.page_ready() {
                self.finish_loading();
            }
            self.clear_pending_resize_paint();
        } else {
            self.place_overlay(&image, geometry);
        }
    }
}

fn accel_geometry(frame: &OsrAccelFrame) -> SurfaceGeometry {
    SurfaceGeometry {
        x: frame.x,
        y: frame.y,
        width: frame.visible_width,
        height: frame.visible_height,
    }
}
