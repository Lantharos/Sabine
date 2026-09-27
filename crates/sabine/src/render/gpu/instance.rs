// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Windows needs D3D12 for shared-texture import and a DirectComposition visual
// for the transparent swapchain. Changing either backend or presentation mode
// can break the browser surface even when ordinary wgpu drawing still works.

use std::sync::{Arc, OnceLock};

use winit::window::Window;

use super::RendererError;

pub(super) struct GpuConnection {
    pub(super) instance: wgpu::Instance,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) adapter: wgpu::Adapter,
}

static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();

pub(super) async fn connect(window: Arc<dyn Window>) -> Result<GpuConnection, RendererError> {
    if let Some(instance) = INSTANCE.get() {
        return request_adapter(instance.clone(), window).await;
    }
    let mut failure = None;
    for backends in backend_preference() {
        let instance = create_instance(*backends);
        match request_adapter(instance.clone(), window.clone()).await {
            Ok(connection) => {
                let _ = INSTANCE.set(instance);
                return Ok(connection);
            }
            Err(error) => failure = Some(error),
        }
    }
    Err(failure.expect("every platform prefers at least one backend"))
}

async fn request_adapter(
    instance: wgpu::Instance,
    window: Arc<dyn Window>,
) -> Result<GpuConnection, RendererError> {
    let surface = instance
        .create_surface(window)
        .map_err(|error| RendererError::Surface(error.to_string()))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        })
        .await
        .map_err(|error| RendererError::Adapter(error.to_string()))?;
    Ok(GpuConnection {
        instance,
        surface,
        adapter,
    })
}

fn create_instance(backends: wgpu::Backends) -> wgpu::Instance {
    let descriptor = wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    };
    #[cfg(target_os = "windows")]
    let descriptor = {
        let mut descriptor = descriptor;
        descriptor.backend_options.dx12.presentation_system =
            wgpu::Dx12SwapchainKind::DxgiFromVisual;
        descriptor
    };
    wgpu::Instance::new(descriptor)
}

fn backend_preference() -> &'static [wgpu::Backends] {
    #[cfg(target_os = "windows")]
    {
        &[wgpu::Backends::DX12]
    }
    #[cfg(target_os = "linux")]
    {
        &[wgpu::Backends::VULKAN, wgpu::Backends::GL]
    }
    #[cfg(target_os = "macos")]
    {
        &[wgpu::Backends::METAL]
    }
}
