// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The Windows shared-texture path must keep its release callback until GPU
// sampling finishes. Its crop UVs describe pixel edges, not pixel centers;
// adding a half-texel inset here resamples the whole page into a blurry mess.

use std::ops::Range;

use crate::render::rect_pipeline::push_image_quad;
use crate::render::{BgraRect, DisplayCommand, DisplayList, ImageId};

use super::{CachedTexture, FrameGeometry, GpuRenderer, RendererError};

pub(super) struct ImageDraw {
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) vertices: Range<u32>,
}

impl GpuRenderer {
    pub(crate) fn remove_image(&mut self, id: &ImageId) {
        #[cfg(any(windows, target_os = "macos"))]
        {
            self.retire_external_texture(id);
            self.external_imports.remove(id);
        }
        self.texture_cache.remove(id);
    }

    pub(crate) fn clear_images(&mut self) {
        #[cfg(any(windows, target_os = "macos"))]
        for (_, completed) in self.external_texture_releases.drain() {
            self.queue.on_submitted_work_done(completed);
            self.submission_poller.notify();
        }
        #[cfg(any(windows, target_os = "macos"))]
        self.external_imports.clear();
        self.texture_cache.clear();
    }

    /// Writes `rects` into the `size` image `id`, replacing the image with a
    /// cleared one first when its size changed.
    pub(crate) fn write_bgra_rects(
        &mut self,
        id: &ImageId,
        size: (u32, u32),
        rects: &[BgraRect<'_>],
    ) -> Result<(), RendererError> {
        self.check_device()?;
        let (width, height) = size;
        if width == 0 || height == 0 {
            return Err(RendererError::Texture(
                "dynamic image has empty size".to_string(),
            ));
        }
        if let Some(rect) = rects.iter().find(|rect| !rect.target.fits(width, height)) {
            return Err(RendererError::Texture(format!(
                "dynamic image region {:?} exceeds {width}x{height}",
                rect.target
            )));
        }
        let reusable = self
            .texture_cache
            .get(id)
            .is_some_and(|entry| !entry.external && entry.width == width && entry.height == height);
        if !reusable {
            self.create_dynamic_bgra_image(id.clone(), width, height);
        }
        let texture = &self.texture_cache[id].texture;
        for rect in rects {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: rect.target.x,
                        y: rect.target.y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                rect.bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: rect.offset,
                    bytes_per_row: Some(rect.bytes_per_row),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: rect.target.width,
                    height: rect.target.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(())
    }

    pub(super) fn collect_images(&self, display_list: &DisplayList, frame: &mut FrameGeometry) {
        let draws = &mut frame.image_draws;
        let vertices = &mut frame.image_vertices;
        draws.clear();
        vertices.clear();
        for command in &display_list.commands {
            let DisplayCommand::Image(image) = command else {
                continue;
            };
            let Some(entry) = self.texture_cache.get(&image.id) else {
                continue;
            };
            let vertex_start = vertices.len() as u32;
            push_image_quad(
                vertices,
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
    }

    pub(super) fn create_dynamic_bgra_image(&mut self, id: ImageId, width: u32, height: u32) {
        #[cfg(any(windows, target_os = "macos"))]
        self.retire_external_texture(&id);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sabine image"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let bind_group = self.image_bind_group(&texture);
        self.texture_cache.insert(
            id,
            CachedTexture {
                texture,
                bind_group,
                origin: (0, 0),
                width,
                height,
                uv_origin: [0.0, 0.0],
                uv_size: [1.0, 1.0],
                external: false,
            },
        );
    }

    pub(super) fn image_bind_group(&self, texture: &wgpu::Texture) -> wgpu::BindGroup {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
