// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// CEF's callback handle is borrowed, not ours to keep. Import only the host's
// copied D3D12 resource on the matching adapter; release its slot after GPU use.
// A valid HANDLE is not a promise that the pixels are still yours.

use wgpu::hal::api::Dx12;
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;

use crate::osr::protocol::OsrAccelFrame;

const CEF_COLOR_TYPE_BGRA_8888: u32 = 1;

/// Open the Sabine-owned D3D12 resource on wgpu's D3D12 device.
///
/// This handle is deliberately not CEF's paint-callback handle. The native
/// host has already copied the frame, completed a D3D11 fence, and duplicated
/// the owned handle into this process. Importing the original handle here or
/// acknowledging the slot while wgpu may still sample it breaks frame lifetime.
pub(crate) fn import_d3d12(
    device: &wgpu::Device,
    frame: &OsrAccelFrame,
) -> Result<wgpu::Texture, String> {
    let handle = frame
        .resource
        .as_ref()
        .ok_or("the browser host did not share this frame's texture")?;
    if frame.coded_width == 0 || frame.coded_height == 0 {
        return Err("invalid d3d11 shared handle frame".into());
    }
    if frame.format != CEF_COLOR_TYPE_BGRA_8888 {
        return Err(format!("unsupported d3d11 color format {}", frame.format));
    }
    let desc = wgpu::TextureDescriptor {
        label: Some("sabine-osr-d3d11"),
        size: wgpu::Extent3d {
            width: frame.coded_width,
            height: frame.coded_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };

    let hal_texture = {
        let Some(hal_device) = (unsafe { device.as_hal::<Dx12>() }) else {
            return Err("wgpu device is not D3D12".into());
        };
        let mut resource = None::<ID3D12Resource>;
        unsafe {
            hal_device
                .raw_device()
                .OpenSharedHandle(handle.raw(), &mut resource)
        }
        .map_err(|error| format!("ID3D12Device::OpenSharedHandle: {error}"))?;
        let resource = resource.ok_or_else(|| "D3D12 shared resource was null".to_string())?;
        unsafe {
            wgpu::hal::dx12::Device::texture_from_raw(
                resource,
                desc.format,
                desc.dimension,
                desc.size,
                desc.mip_level_count,
                desc.sample_count,
            )
        }
    };

    Ok(unsafe {
        device.create_texture_from_hal::<Dx12>(hal_texture, &desc, wgpu::TextureUses::empty())
    })
}
