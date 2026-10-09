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
pub(crate) use iosurface::{SharedSurface, SurfaceBroker, SurfaceRegistry};

use crate::osr::protocol::OsrAccelFrame;

#[cfg(windows)]
pub(crate) type SharedResource = std::sync::Arc<d3d12::SharedHandle>;
#[cfg(target_os = "linux")]
pub(crate) type SharedResource = std::sync::Arc<Dmabuf>;
#[cfg(target_os = "macos")]
pub(crate) type SharedResource = SharedSurface;

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
