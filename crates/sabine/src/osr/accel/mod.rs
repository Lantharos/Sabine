#[cfg(windows)]
mod d3d12;
#[cfg(target_os = "macos")]
mod iosurface;

#[cfg(windows)]
pub(crate) use d3d12::{adapter_luid, close_imported_handle};
#[cfg(target_os = "macos")]
pub(crate) use iosurface::{SharedSurface, SurfaceBroker, SurfaceRegistry};

use crate::osr::protocol::OsrAccelFrame;
use crate::render::GpuRenderer;

pub(crate) fn import_texture(
    renderer: &GpuRenderer,
    frame: &OsrAccelFrame,
) -> Result<wgpu::Texture, String> {
    #[cfg(windows)]
    {
        d3d12::try_import_d3d12(renderer, frame)
    }
    #[cfg(target_os = "macos")]
    {
        iosurface::import_io_surface(renderer, frame)
    }
}

pub(crate) fn install_imported_texture(
    renderer: &mut GpuRenderer,
    texture_id: &str,
    frame: &OsrAccelFrame,
    texture: wgpu::Texture,
    completed: impl FnOnce() + Send + 'static,
) -> Result<(), String> {
    renderer
        .set_external_bgra_texture(
            texture_id,
            texture,
            (frame.visible_x, frame.visible_y),
            (frame.visible_width, frame.visible_height),
            completed,
        )
        .map_err(|error| error.to_string())
}
