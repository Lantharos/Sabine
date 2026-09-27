// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The Windows shared-texture path must keep its release callback until GPU
// sampling finishes. Its crop UVs describe pixel edges, not pixel centers;
// adding a half-texel inset here resamples the whole page into a blurry mess.

use std::ops::Range;

use crate::render::rect_pipeline::{ImageVertex, push_image_quad};
use crate::render::{DisplayCommand, DisplayList, PixelRect};

use super::{CachedTexture, GpuRenderer, RendererError};

pub(super) struct ImageDraw {
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) vertices: Range<u32>,
}

impl GpuRenderer {
    pub(crate) fn remove_image(&mut self, id: &str) {
        #[cfg(windows)]
        self.retire_external_texture(id);
        self.texture_cache.remove(id);
    }

    pub(crate) fn clear_images(&mut self) {
        #[cfg(windows)]
        for (_, completed) in self.external_texture_releases.drain() {
            self.queue.on_submitted_work_done(completed);
            self.submission_poller.notify();
        }
        self.texture_cache.clear();
    }

    pub(crate) fn upload_bgra_regions(
        &mut self,
        id: &str,
        size: (u32, u32),
        bytes: &[u8],
        regions: &[PixelRect],
    ) -> Result<(), RendererError> {
        self.check_device()?;
        let (width, height) = size;
        if width == 0 || height == 0 {
            return Err(RendererError::Texture(
                "dynamic image has empty size".to_string(),
            ));
        }
        let expected_len = width as usize * height as usize * 4;
        if bytes.len() != expected_len {
            return Err(RendererError::Texture(format!(
                "dynamic image expected {expected_len} bytes, got {}",
                bytes.len()
            )));
        }
        if let Some(region) = regions.iter().find(|region| !region.fits(width, height)) {
            return Err(RendererError::Texture(format!(
                "dynamic image region {region:?} exceeds {width}x{height}"
            )));
        }
        let reusable = self
            .texture_cache
            .get(id)
            .is_some_and(|entry| !entry.external && entry.width == width && entry.height == height);
        if !reusable {
            self.create_dynamic_bgra_image(id.to_string(), width, height);
        }
        let entire = [PixelRect {
            x: 0,
            y: 0,
            width,
            height,
        }];
        let regions = if reusable { regions } else { &entire };
        let texture = &self.texture_cache[id].texture;
        for region in regions.iter().filter(|region| region.area() > 0) {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: region.x,
                        y: region.y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: (u64::from(region.y) * u64::from(width) + u64::from(region.x)) * 4,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width: region.width,
                    height: region.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(())
    }

    pub(super) fn image_draws(
        &self,
        display_list: &DisplayList,
    ) -> (Vec<ImageDraw>, Vec<ImageVertex>) {
        let mut draws = Vec::new();
        let mut vertices = Vec::new();
        for command in &display_list.commands {
            let DisplayCommand::Image(image) = command else {
                continue;
            };
            let Some(entry) = self.texture_cache.get(&image.id) else {
                continue;
            };
            let vertex_start = vertices.len() as u32;
            push_image_quad(
                &mut vertices,
                [image.x, image.y, image.width, image.height],
                self.scale_factor,
                entry.uv_origin,
                entry.uv_size,
            );
            draws.push(ImageDraw {
                bind_group: entry.bind_group.clone(),
                vertices: vertex_start..vertices.len() as u32,
            });
        }
        (draws, vertices)
    }

    #[cfg(windows)]
    pub fn set_external_bgra_texture(
        &mut self,
        id: impl Into<String>,
        texture: wgpu::Texture,
        source_origin: (u32, u32),
        size: (u32, u32),
        completed: impl FnOnce() + Send + 'static,
    ) -> Result<(), RendererError> {
        self.check_device()?;
        let id = id.into();
        let (width, height) = size;
        if width == 0 || height == 0 {
            completed();
            return Err(RendererError::Texture(
                "external image has empty size".to_string(),
            ));
        }
        let source_width = texture.width();
        let source_height = texture.height();
        if source_origin.0.saturating_add(width) > source_width
            || source_origin.1.saturating_add(height) > source_height
        {
            completed();
            return Err(RendererError::Texture(format!(
                "external image region {},{} {width}x{height} exceeds {source_width}x{source_height}",
                source_origin.0, source_origin.1
            )));
        }
        self.retire_external_texture(&id);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&id),
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
        });
        self.texture_cache.insert(
            id.clone(),
            CachedTexture {
                texture,
                bind_group,
                width,
                height,
                uv_origin: [
                    source_origin.0 as f32 / source_width as f32,
                    source_origin.1 as f32 / source_height as f32,
                ],
                uv_size: [
                    width as f32 / source_width as f32,
                    height as f32 / source_height as f32,
                ],
                external: true,
            },
        );
        self.external_texture_releases
            .insert(id, Box::new(completed));
        Ok(())
    }

    #[cfg(windows)]
    fn retire_external_texture(&mut self, id: &str) {
        if let Some(completed) = self.external_texture_releases.remove(id) {
            self.queue.on_submitted_work_done(completed);
            self.submission_poller.notify();
        }
    }

    #[cfg(windows)]
    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub(super) fn create_dynamic_bgra_image(&mut self, id: String, width: u32, height: u32) {
        #[cfg(windows)]
        self.retire_external_texture(&id);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&id),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&id),
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
        });
        self.texture_cache.insert(
            id,
            CachedTexture {
                texture,
                bind_group,
                width,
                height,
                uv_origin: [0.0, 0.0],
                uv_size: [1.0, 1.0],
                external: false,
            },
        );
    }
}
