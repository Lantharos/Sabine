use std::{collections::HashMap, sync::Arc};

use thiserror::Error;
use winit::window::Window;

use crate::render::rect_pipeline::to_wgpu_color;
use crate::render::{DisplayCommand, DisplayList, ImageId};

mod device;
mod draw;
mod images;
mod surface;

pub(crate) use images::ExternalSlot;

use device::{health, instance, retirement};
use draw::Pipelines;
use draw::geometry::{Draw, FrameGeometry};
use draw::text::TextRendererState;
use draw::vertex_buffer::DynamicVertexBuffer;
use surface::{select_present_mode, select_surface_alpha_mode};

#[derive(Debug, Error)]
pub enum RendererError {
    #[error("failed to create GPU surface: {0}")]
    Surface(String),
    #[error("failed to request GPU adapter: {0}")]
    Adapter(String),
    #[error("failed to request GPU device: {0}")]
    Device(String),
    #[error("GPU device was lost: {0}")]
    DeviceLost(String),
    #[error("text renderer failed: {0}")]
    Text(String),
    #[error("surface validation failed")]
    SurfaceValidation,
    #[error("texture update failed: {0}")]
    Texture(String),
}

pub struct GpuRenderer {
    instance: wgpu::Instance,
    health: Arc<health::DeviceHealth>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    source: instance::SurfaceSource,
    surface_config: wgpu::SurfaceConfiguration,
    pipelines: Pipelines,
    rect_vertex_buffer: DynamicVertexBuffer,
    image_vertex_buffer: DynamicVertexBuffer,
    frame: FrameGeometry,
    text: Option<TextRendererState>,
    texture_cache: HashMap<ImageId, CachedTexture>,
    external_texture_releases: HashMap<ImageId, Box<dyn FnOnce() + Send + 'static>>,
    external_imports: HashMap<ImageId, Vec<images::ImportedTexture>>,
    #[cfg(target_os = "linux")]
    producer_waits: Option<device::ProducerWaits>,
    submission_poller: retirement::SubmissionPoller,
    #[cfg(windows)]
    software_adapter: bool,
    scale_factor: f32,
    #[cfg(target_os = "macos")]
    present_modes: Vec<wgpu::PresentMode>,
    surface_alpha_is_opaque: bool,
    window: Arc<dyn Window>,
}

pub(super) struct CachedTexture {
    pub(super) texture: wgpu::Texture,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) origin: (u32, u32),
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) uv_origin: [f32; 2],
    pub(super) uv_size: [f32; 2],
    pub(super) external: bool,
}

/// A renderer about to start, with what had to be made on the window's
/// thread. It starts on any thread.
pub(crate) struct RendererStart {
    window: Arc<dyn Window>,
    source: instance::SurfaceSource,
    connecting: instance::Connecting,
}

impl GpuRenderer {
    /// Prepares a renderer for the window, on the window's thread. A renderer
    /// replacing one whose device was lost looks at the adapters the system
    /// has now.
    pub(crate) fn prepare(
        window: Arc<dyn Window>,
        after_device_loss: bool,
    ) -> Result<RendererStart, RendererError> {
        let source = instance::SurfaceSource::new(window.clone())?;
        let connecting = instance::prepare(&source, after_device_loss)?;
        Ok(RendererStart {
            window,
            source,
            connecting,
        })
    }

    pub(crate) async fn start(
        RendererStart {
            window,
            source,
            connecting,
        }: RendererStart,
        transparent: bool,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, RendererError> {
        let connection = instance::connect(&source, connecting).await?;
        Self::connected(window, source, connection, transparent, wake).await
    }

    async fn connected(
        window: Arc<dyn Window>,
        source: instance::SurfaceSource,
        instance::GpuConnection {
            instance,
            surface,
            adapter,
        }: instance::GpuConnection,
        transparent: bool,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, RendererError> {
        let size = window.surface_size();
        let (device, queue) = request_device(&adapter).await?;

        let health = health::DeviceHealth::watch(&device, wake);
        let submission_poller = retirement::SubmissionPoller::new(&device, &queue, health.clone())?;
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let present_mode = select_present_mode(&capabilities.present_modes, false);
        let alpha_mode = select_surface_alpha_mode(&capabilities.alpha_modes, transparent);
        let surface_alpha_is_opaque = matches!(
            alpha_mode,
            wgpu::CompositeAlphaMode::Opaque | wgpu::CompositeAlphaMode::Auto
        );
        if crate::launch::metrics::trace_enabled() {
            #[cfg(target_os = "windows")]
            let presentation = "DirectComposition";
            #[cfg(not(target_os = "windows"))]
            let presentation = "native";
            let adapter_info = adapter.get_info();
            eprintln!(
                "Sabine GPU: adapter={:?} type={:?} backend={:?} driver={:?} presentation={presentation} surface alpha={alpha_mode:?} transparent={transparent}",
                adapter_info.name,
                adapter_info.device_type,
                adapter_info.backend,
                adapter_info.driver
            );
        }
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            color_space: wgpu::SurfaceColorSpace::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &surface_config);
        #[cfg(windows)]
        source.commit();
        let pipelines = Pipelines::new(
            &device,
            format,
            [surface_config.width as f32, surface_config.height as f32],
        );

        Ok(Self {
            instance,
            health,
            #[cfg(target_os = "linux")]
            producer_waits: device::ProducerWaits::new(&device, &queue),
            device,
            queue,
            surface,
            surface_config,
            pipelines,
            rect_vertex_buffer: DynamicVertexBuffer::default(),
            image_vertex_buffer: DynamicVertexBuffer::default(),
            frame: FrameGeometry::default(),
            text: None,
            texture_cache: HashMap::new(),
            external_texture_releases: HashMap::new(),
            external_imports: HashMap::new(),
            submission_poller,
            #[cfg(windows)]
            software_adapter: adapter.get_info().device_type == wgpu::DeviceType::Cpu,
            scale_factor: window.scale_factor() as f32,
            #[cfg(target_os = "macos")]
            present_modes: capabilities.present_modes,
            surface_alpha_is_opaque,
            source,
            window,
        })
    }

    pub(crate) fn surface_alpha_is_opaque(&self) -> bool {
        self.surface_alpha_is_opaque
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn imports_dmabuf(&self) -> bool {
        self.device
            .features()
            .contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF)
    }

    #[cfg(windows)]
    pub(crate) fn uses_software_adapter(&self) -> bool {
        self.software_adapter
    }

    pub fn device_loss(&self) -> Option<String> {
        self.health.failure()
    }

    /// Whether a failure came from a lost device. A loss is only reported
    /// once wgpu next waits on the device, so this polls it first.
    pub(crate) fn lost_device(&self) -> bool {
        let _ = self.device.poll(wgpu::PollType::Poll);
        self.device_loss().is_some()
    }

    /// The width `text` takes on one line at `size` logical pixels.
    pub(crate) fn measure_text(&mut self, text: &str, size: f32) -> f32 {
        self.text
            .get_or_insert_with(|| {
                TextRendererState::new(&self.device, &self.queue, self.surface_config.format)
            })
            .measure(text, size)
    }

    fn check_device(&self) -> Result<(), RendererError> {
        self.device_loss()
            .map_or(Ok(()), |message| Err(RendererError::DeviceLost(message)))
    }

    pub fn render(&mut self, display_list: &DisplayList) -> Result<(), RendererError> {
        self.check_device()?;
        self.pipelines.set_viewport(
            &self.queue,
            [
                self.surface_config.width as f32,
                self.surface_config.height as f32,
            ],
        );
        let mut frame = std::mem::take(&mut self.frame);
        let result = self.render_frame(display_list, &mut frame);
        self.frame = frame;
        result
    }

    fn render_frame(
        &mut self,
        display_list: &DisplayList,
        geometry: &mut FrameGeometry,
    ) -> Result<(), RendererError> {
        self.collect_geometry(display_list, geometry);
        if geometry.has_cutouts() {
            self.pipelines.prepare_cutouts(&self.device);
        }
        let has_text = display_list
            .commands
            .iter()
            .any(|command| matches!(command, DisplayCommand::Text(_)));
        if has_text {
            let text = self.text.get_or_insert_with(|| {
                TextRendererState::new(&self.device, &self.queue, self.surface_config.format)
            });
            text.prepare(
                &self.device,
                &self.queue,
                display_list,
                self.scale_factor,
                self.surface_config.width,
                self.surface_config.height,
            )?;
        }

        let Some(frame) = self.acquire_surface_texture()? else {
            return Ok(());
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sabine render encoder"),
            });
        let rect_vertex_buffer = self.rect_vertex_buffer.upload(
            &self.device,
            &self.queue,
            "sabine rounded rect vertices",
            &geometry.rect_vertices,
        );
        let image_vertex_buffer = self.image_vertex_buffer.upload(
            &self.device,
            &self.queue,
            "sabine image vertices",
            &geometry.image_vertices,
        );

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sabine render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(to_wgpu_color(display_list.background)),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_bind_group(0, &self.pipelines.globals_bind_group, &[]);
            for draw in &geometry.draws {
                match draw {
                    Draw::Rects { vertices, cutout } => {
                        let Some(vertex_buffer) = &rect_vertex_buffer else {
                            continue;
                        };
                        pass.set_pipeline(self.pipelines.rects(*cutout));
                        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        pass.draw(vertices.clone(), 0..1);
                    }
                    Draw::Image(image) => {
                        let Some(vertex_buffer) = &image_vertex_buffer else {
                            continue;
                        };
                        pass.set_pipeline(&self.pipelines.image);
                        pass.set_bind_group(1, &image.bind_group, &[]);
                        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                        pass.draw(image.vertices.clone(), 0..1);
                    }
                }
            }

            if has_text && let Some(text) = &self.text {
                text.render(&mut pass)?;
            }
        }

        self.queue.submit(Some(encoder.finish()));
        #[cfg(target_os = "linux")]
        if self
            .producer_waits
            .as_mut()
            .is_some_and(device::ProducerWaits::submitted)
        {
            self.submission_poller.notify();
        }
        self.window.pre_present_notify();
        self.queue.present(frame);
        if let Some(text) = &mut self.text {
            text.trim();
        }
        Ok(())
    }
}

async fn request_device(
    adapter: &wgpu::Adapter,
) -> Result<(wgpu::Device, wgpu::Queue), RendererError> {
    #[cfg(target_os = "linux")]
    let required_features = adapter.features() & wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF;
    #[cfg(not(target_os = "linux"))]
    let required_features = wgpu::Features::empty();
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("sabine-gpu"),
        required_features,
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        ..Default::default()
    };
    #[cfg(target_os = "linux")]
    {
        device::open_device(adapter, &descriptor).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        adapter
            .request_device(&descriptor)
            .await
            .map_err(|error| RendererError::Device(error.to_string()))
    }
}

impl Drop for GpuRenderer {
    fn drop(&mut self) {
        if self.external_texture_releases.is_empty() {
            return;
        }
        let releases = self
            .external_texture_releases
            .drain()
            .map(|(_, release)| release)
            .collect::<Vec<_>>();
        self.queue.on_submitted_work_done(move || {
            for release in releases {
                release();
            }
        });
        self.submission_poller.notify();
    }
}
