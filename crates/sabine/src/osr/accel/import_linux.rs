use std::os::fd::{FromRawFd, OwnedFd};

use wgpu::hal::api::Vulkan;

use crate::osr::protocol::OsrAccelFrame;
use crate::render::GpuRenderer;

const CEF_COLOR_TYPE_BGRA_8888: u32 = 1;

/// Zero-copy DMA-BUF → wgpu via Vulkan `VK_EXT_external_memory_dma_buf`.
///
/// On failure the caller keeps `frame.fd` for [`super::mmap_fallback`].
pub(crate) fn try_import_dmabuf(
    renderer: &mut GpuRenderer,
    frame: &OsrAccelFrame,
) -> Result<wgpu::Texture, String> {
    if frame.fd < 0 || frame.width == 0 || frame.height == 0 {
        return Err("invalid dma-buf frame".into());
    }
    if frame.format != CEF_COLOR_TYPE_BGRA_8888 {
        return Err("dma-buf import requires BGRA8888".into());
    }
    if !renderer.supports_feature(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF) {
        return Err("adapter lacks VULKAN_EXTERNAL_MEMORY_DMA_BUF".into());
    }

    let desc = wgpu::TextureDescriptor {
        label: Some("sabine-osr-dmabuf"),
        size: wgpu::Extent3d {
            width: frame.width,
            height: frame.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };

    let dup = unsafe { libc::dup(frame.fd) };
    if dup < 0 {
        return Err(format!(
            "dup dma-buf failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let owned = unsafe { OwnedFd::from_raw_fd(dup) };

    let hal_desc = wgpu::hal::TextureDescriptor {
        label: desc.label,
        size: desc.size,
        mip_level_count: desc.mip_level_count,
        sample_count: desc.sample_count,
        dimension: desc.dimension,
        format: desc.format,
        usage: wgpu::TextureUses::RESOURCE | wgpu::TextureUses::COPY_SRC,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: vec![],
    };

    let hal_texture = {
        let Some(hal_device) = (unsafe { renderer.device().as_hal::<Vulkan>() }) else {
            return Err("wgpu device is not Vulkan".into());
        };
        unsafe {
            hal_device
                .texture_from_dmabuf_fd(
                    owned,
                    &hal_desc,
                    frame.modifier,
                    u64::from(frame.stride),
                    frame.offset,
                )
                .map_err(|error| format!("texture_from_dmabuf_fd: {error:?}"))?
        }
    };

    Ok(unsafe {
        let texture = renderer.device().create_texture_from_hal::<Vulkan>(
            hal_texture,
            &desc,
            wgpu::TextureUses::RESOURCE,
        );
        // Vulkan owns the duplicated fd; close the IPC copy.
        libc::close(frame.fd);
        texture
    })
}
