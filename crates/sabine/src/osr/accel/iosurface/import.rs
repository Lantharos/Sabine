use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTextureDescriptor, MTLTextureType,
    MTLTextureUsage,
};
use wgpu::hal::api::Metal;

use crate::osr::protocol::OsrAccelFrame;
use crate::render::GpuRenderer;

const CEF_COLOR_TYPE_RGBA_8888: u32 = 0;
const CEF_COLOR_TYPE_BGRA_8888: u32 = 1;

pub(crate) fn import_io_surface(
    renderer: &GpuRenderer,
    frame: &OsrAccelFrame,
) -> Result<wgpu::Texture, String> {
    let surface = frame
        .io_surface
        .as_ref()
        .ok_or("the browser host did not share this frame's surface")?;
    let (format, pixel_format) = match frame.format {
        CEF_COLOR_TYPE_BGRA_8888 => (
            wgpu::TextureFormat::Bgra8UnormSrgb,
            MTLPixelFormat::BGRA8Unorm_sRGB,
        ),
        CEF_COLOR_TYPE_RGBA_8888 => (
            wgpu::TextureFormat::Rgba8UnormSrgb,
            MTLPixelFormat::RGBA8Unorm_sRGB,
        ),
        other => return Err(format!("unsupported shared surface color format {other}")),
    };
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
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    let hal_texture = {
        let device =
            unsafe { renderer.device().as_hal::<Metal>() }.ok_or("wgpu device is not Metal")?;
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
        let texture = device
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
        renderer.device().create_texture_from_hal::<Metal>(
            hal_texture,
            &desc,
            wgpu::TextureUses::empty(),
        )
    })
}
