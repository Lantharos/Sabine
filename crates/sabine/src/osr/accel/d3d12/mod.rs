mod handles;
mod import;

pub(crate) use handles::{SharedHandle, SharedHandles};
pub(super) use import::import_d3d12;

use wgpu::hal::api::Dx12;

use crate::render::GpuRenderer;

pub(crate) fn adapter_luid(renderer: &GpuRenderer) -> String {
    let device =
        unsafe { renderer.device().as_hal::<Dx12>() }.expect("Windows OSR uses a D3D12 device");
    let luid = unsafe { device.raw_device().GetAdapterLuid() };
    format!("{},{}", luid.HighPart, luid.LowPart)
}
