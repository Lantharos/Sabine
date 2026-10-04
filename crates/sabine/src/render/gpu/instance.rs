// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Windows needs D3D12 for shared-texture import and presents into the page
// visual of Sabine's own DirectComposition tree, which keeps the swapchain
// transparent and lets media visuals sit beneath it. Changing either backend or
// presentation mode can break the browser surface even when ordinary wgpu
// drawing still works.

use std::sync::{Arc, OnceLock};

use winit::window::Window;

use super::RendererError;
#[cfg(windows)]
use crate::render::Composition;

pub(super) struct GpuConnection {
    pub(super) instance: wgpu::Instance,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) adapter: wgpu::Adapter,
}

/// What the renderer presents into: the window itself, or on Windows the
/// page visual of the window's composition tree.
pub(super) struct SurfaceSource {
    #[cfg(not(windows))]
    window: Arc<dyn Window>,
    #[cfg(windows)]
    composition: Arc<Composition>,
}

impl SurfaceSource {
    pub(super) fn new(window: Arc<dyn Window>) -> Result<Self, RendererError> {
        Ok(Self {
            #[cfg(windows)]
            composition: Composition::for_window(window.as_ref())
                .map_err(RendererError::Surface)?,
            #[cfg(not(windows))]
            window,
        })
    }

    pub(super) fn create(
        &self,
        instance: &wgpu::Instance,
    ) -> Result<wgpu::Surface<'static>, RendererError> {
        #[cfg(windows)]
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CompositionVisual(
                self.composition.page(),
            ))
        };
        #[cfg(not(windows))]
        let surface = instance.create_surface(self.window.clone());
        surface.map_err(|error| RendererError::Surface(error.to_string()))
    }

    /// Shows a surface whose swapchain was just created.
    #[cfg(windows)]
    pub(super) fn commit(&self) {
        self.composition.commit();
    }
}

static INSTANCE: OnceLock<wgpu::Instance> = OnceLock::new();

pub(super) async fn connect(source: &SurfaceSource) -> Result<GpuConnection, RendererError> {
    if let Some(instance) = INSTANCE.get() {
        return request_adapter(instance.clone(), source).await;
    }
    let mut failure = None;
    for backends in backend_preference() {
        let instance = create_instance(*backends);
        match request_adapter(instance.clone(), source).await {
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
    source: &SurfaceSource,
) -> Result<GpuConnection, RendererError> {
    let surface = source.create(&instance)?;
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
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    })
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
