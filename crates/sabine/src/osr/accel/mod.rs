mod import_win;

use crate::osr::protocol::OsrAccelFrame;
use crate::render::GpuRenderer;

pub(crate) use import_win::{adapter_luid, close_imported_handle, try_import_d3d12};

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
