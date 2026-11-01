use wgpu::util::DeviceExt;

use crate::render::rect_pipeline::{
    CUTOUT_BLENDING, Globals, create_image_pipeline, create_rounded_rect_pipeline,
};

/// What the window's frames are drawn with: the pipelines, the sampler and
/// layout images bind with, and the viewport every vertex shader reads.
pub(in crate::render::gpu) struct Pipelines {
    format: wgpu::TextureFormat,
    pub(in crate::render::gpu) rounded_rect: wgpu::RenderPipeline,
    cutout: Option<wgpu::RenderPipeline>,
    pub(in crate::render::gpu) image: wgpu::RenderPipeline,
    image_sampler: wgpu::Sampler,
    image_bind_group_layout: wgpu::BindGroupLayout,
    globals_buffer: wgpu::Buffer,
    globals_bind_group_layout: wgpu::BindGroupLayout,
    pub(in crate::render::gpu) globals_bind_group: wgpu::BindGroup,
    viewport: [f32; 2],
}

impl Pipelines {
    pub(in crate::render::gpu) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport: [f32; 2],
    ) -> Self {
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sabine globals"),
            contents: bytemuck::cast_slice(&[globals(viewport)]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let globals_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sabine globals bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sabine globals bind group"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sabine image sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let image_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sabine image per-texture bind group layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });
        Self {
            format,
            rounded_rect: create_rounded_rect_pipeline(
                device,
                format,
                &globals_bind_group_layout,
                wgpu::BlendState::ALPHA_BLENDING,
            ),
            cutout: None,
            image: create_image_pipeline(device, format, &globals_bind_group_layout),
            image_sampler,
            image_bind_group_layout,
            globals_buffer,
            globals_bind_group_layout,
            globals_bind_group,
            viewport,
        }
    }

    /// Updates the viewport the shaders map pixels with, once it changed.
    pub(in crate::render::gpu) fn set_viewport(&mut self, queue: &wgpu::Queue, viewport: [f32; 2]) {
        if self.viewport != viewport {
            self.viewport = viewport;
            queue.write_buffer(
                &self.globals_buffer,
                0,
                bytemuck::cast_slice(&[globals(viewport)]),
            );
        }
    }

    /// Creates the pipeline that punches holes for media, the first time a
    /// frame has one.
    pub(in crate::render::gpu) fn prepare_cutouts(&mut self, device: &wgpu::Device) {
        if self.cutout.is_none() {
            self.cutout = Some(create_rounded_rect_pipeline(
                device,
                self.format,
                &self.globals_bind_group_layout,
                CUTOUT_BLENDING,
            ));
        }
    }

    pub(in crate::render::gpu) fn rects(&self, cutout: bool) -> &wgpu::RenderPipeline {
        match &self.cutout {
            Some(cutout_pipeline) if cutout => cutout_pipeline,
            _ => &self.rounded_rect,
        }
    }

    pub(in crate::render::gpu) fn image_bind_group(
        &self,
        device: &wgpu::Device,
        texture: &wgpu::Texture,
    ) -> wgpu::BindGroup {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sabine image"),
            layout: &self.image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.image_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        })
    }
}

fn globals(viewport: [f32; 2]) -> Globals {
    Globals {
        viewport,
        _padding: [0.0, 0.0],
    }
}
