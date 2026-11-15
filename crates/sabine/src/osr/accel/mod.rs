#[cfg(windows)]
mod d3d12;
#[cfg(target_os = "linux")]
mod dmabuf;
#[cfg(target_os = "macos")]
mod iosurface;

#[cfg(windows)]
pub(crate) use d3d12::{SharedHandles, adapter_luid};
#[cfg(target_os = "linux")]
pub(crate) use dmabuf::{Dmabuf, Dmabufs};
#[cfg(target_os = "macos")]
pub(crate) use iosurface::{SharedSurface, SurfaceBroker, SurfaceCache, SurfaceRegistry};

use crate::osr::protocol::OsrAccelFrame;

#[cfg(windows)]
pub(crate) type SharedResource = std::sync::Arc<d3d12::SharedHandle>;
#[cfg(target_os = "linux")]
pub(crate) type SharedResource = std::sync::Arc<Dmabuf>;
#[cfg(target_os = "macos")]
pub(crate) type SharedResource = SharedSurface;

const CEF_COLOR_TYPE_RGBA_8888: u32 = 0;
const CEF_COLOR_TYPE_BGRA_8888: u32 = 1;

/// The texture format of a frame in the color type CEF painted it in.
pub(crate) fn texture_format(color_type: u32) -> Result<wgpu::TextureFormat, String> {
    match color_type {
        CEF_COLOR_TYPE_BGRA_8888 => Ok(wgpu::TextureFormat::Bgra8UnormSrgb),
        CEF_COLOR_TYPE_RGBA_8888 => Ok(wgpu::TextureFormat::Rgba8UnormSrgb),
        other => Err(format!("unsupported frame color format {other}")),
    }
}

pub(crate) fn import_texture(
    device: &wgpu::Device,
    frame: &OsrAccelFrame,
) -> Result<wgpu::Texture, String> {
    #[cfg(windows)]
    {
        d3d12::import_d3d12(device, frame)
    }
    #[cfg(target_os = "linux")]
    {
        dmabuf::import_dmabuf(device, frame)
    }
    #[cfg(target_os = "macos")]
    {
        iosurface::import_io_surface(device, frame)
    }
}
