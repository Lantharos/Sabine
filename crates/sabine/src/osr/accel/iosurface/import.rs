use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTextureDescriptor, MTLTextureType,
    MTLTextureUsage,
};
use wgpu::hal::api::Metal;

use crate::osr::accel::texture_format;
use crate::osr::protocol::OsrAccelFrame;

pub(crate) fn import_io_surface(
    device: &wgpu::Device,
    frame: &OsrAccelFrame,
) -> Result<wgpu::Texture, String> {
    let surface = frame
        .resource
        .as_ref()
        .ok_or("the browser host did not share this frame's surface")?;
    let format = texture_format(frame.format)?;
    let pixel_format = if format == wgpu::TextureFormat::Bgra8UnormSrgb {
        MTLPixelFormat::BGRA8Unorm_sRGB
    } else {
        MTLPixelFormat::RGBA8Unorm_sRGB
    };
    let (surface_width, surface_height) = (surface.width(), surface.height());
    if surface_width != frame.coded_width as usize || surface_height != frame.coded_height as usize
    {
        return Err(format!(
            "the shared surface is {surface_width}x{surface_height}, not the {}x{} its frame names",
            frame.coded_width, frame.coded_height
        ));
    }
    let desc = wgpu::TextureDescriptor {
        label: Some("sabine-osr-iosurface"),
        size: wgpu::Extent3d {
            width: frame.coded_width,
            height: frame.coded_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    let hal_texture = {
        let hal_device = unsafe { device.as_hal::<Metal>() }.ok_or("wgpu device is not Metal")?;
        let descriptor = unsafe {
            MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                pixel_format,
                frame.coded_width as usize,
                frame.coded_height as usize,
                false,
            )
        };
        descriptor.setUsage(MTLTextureUsage::ShaderRead);
        descriptor.setStorageMode(MTLStorageMode::Shared);
        let texture = hal_device
            .raw_device()
            .newTextureWithDescriptor_iosurface_plane(&descriptor, surface, 0)
            .ok_or("Metal could not wrap the shared surface")?;
        unsafe {
            wgpu::hal::metal::Device::texture_from_raw(
                texture,
                format,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent {
                    width: frame.coded_width,
                    height: frame.coded_height,
                    depth: 1,
                },
                None,
            )
        }
    };
    Ok(unsafe {
        device.create_texture_from_hal::<Metal>(hal_texture, &desc, wgpu::TextureUses::empty())
    })
}
