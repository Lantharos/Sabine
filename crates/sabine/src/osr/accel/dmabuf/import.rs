use wgpu::hal::api::Vulkan;

use crate::osr::protocol::OsrAccelFrame;

const CEF_COLOR_TYPE_RGBA_8888: u32 = 0;
const CEF_COLOR_TYPE_BGRA_8888: u32 = 1;

/// Wrap the host's copy of a frame as a texture on wgpu's Vulkan device,
/// sampling the shared memory directly.
pub(crate) fn import_dmabuf(
    device: &wgpu::Device,
    frame: &OsrAccelFrame,
) -> Result<wgpu::Texture, String> {
    let dmabuf = frame
        .resource
        .as_ref()
        .ok_or("the browser host did not share this frame's dma-buf")?;
    let format = match frame.format {
        CEF_COLOR_TYPE_BGRA_8888 => wgpu::TextureFormat::Bgra8UnormSrgb,
        CEF_COLOR_TYPE_RGBA_8888 => wgpu::TextureFormat::Rgba8UnormSrgb,
        other => return Err(format!("unsupported dma-buf color format {other}")),
    };
    let size = wgpu::Extent3d {
        width: dmabuf.width,
        height: dmabuf.height,
        depth_or_array_layers: 1,
    };
    let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC;
    let fd = dmabuf
        .fd
        .try_clone()
        .map_err(|error| format!("could not duplicate the dma-buf: {error}"))?;
    let hal_texture = {
        let hal_device = unsafe { device.as_hal::<Vulkan>() }.ok_or("wgpu device is not Vulkan")?;
        unsafe {
            hal_device.texture_from_dmabuf_fd(
                fd,
                &wgpu::hal::TextureDescriptor {
                    label: Some("sabine-osr-dmabuf"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUses::RESOURCE | wgpu::TextureUses::COPY_SRC,
                    memory_flags: wgpu::hal::MemoryFlags::empty(),
                    view_formats: Vec::new(),
                },
                dmabuf.modifier,
                u64::from(dmabuf.stride),
                u64::from(dmabuf.offset),
            )
        }
        .map_err(|error| format!("Vulkan could not import the dma-buf: {error}"))?
    };
    Ok(unsafe {
        device.create_texture_from_hal::<Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("sabine-osr-dmabuf"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            },
            wgpu::TextureUses::empty(),
        )
    })
}
