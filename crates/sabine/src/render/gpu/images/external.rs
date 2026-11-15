// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The Windows shared-texture path must keep its release callback until GPU
// sampling finishes. Its crop UVs describe pixel edges, not pixel centers;
// adding a half-texel inset here resamples the whole page into a blurry mess.

use crate::osr::control::SlotRelease;
use crate::render::ImageId;
use crate::render::gpu::{CachedTexture, GpuRenderer, RendererError};

/// The producer slot an external frame was copied into, the texture that
/// slot currently holds, and the size the frame says that texture has.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ExternalSlot {
    pub index: u32,
    pub resource_id: u64,
    pub size: (u32, u32),
}

#[derive(Clone)]
pub(in crate::render::gpu) struct ImportedTexture {
    slot: u32,
    resource_id: u64,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl GpuRenderer {
    /// Show a frame from a producer-owned texture. Each slot's texture is
    /// imported once and reused until the producer replaces or retires it,
    /// or a frame names it with another size, which `import` has to confirm.
    pub(crate) fn set_external_bgra_texture(
        &mut self,
        id: &ImageId,
        slot: ExternalSlot,
        import: impl FnOnce(&wgpu::Device) -> Result<wgpu::Texture, String>,
        source_origin: (u32, u32),
        size: (u32, u32),
        release: SlotRelease,
    ) -> Result<(), RendererError> {
        self.check_device()?;
        let imported = self.imported_texture(id, slot, import)?;
        let (width, height) = size;
        if width == 0 || height == 0 {
            return Err(RendererError::Texture(
                "external image has empty size".to_string(),
            ));
        }
        let source_width = imported.texture.width();
        let source_height = imported.texture.height();
        if source_origin.0.saturating_add(width) > source_width
            || source_origin.1.saturating_add(height) > source_height
        {
            return Err(RendererError::Texture(format!(
                "external image region {},{} {width}x{height} exceeds {source_width}x{source_height}",
                source_origin.0, source_origin.1
            )));
        }
        self.retire_external_texture(id);
        self.texture_cache.insert(
            id.clone(),
            CachedTexture {
                texture: imported.texture,
                bind_group: imported.bind_group,
                origin: source_origin,
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
        self.external_texture_releases.insert(id.clone(), release);
        Ok(())
    }

    fn imported_texture(
        &mut self,
        id: &ImageId,
        slot: ExternalSlot,
        import: impl FnOnce(&wgpu::Device) -> Result<wgpu::Texture, String>,
    ) -> Result<ImportedTexture, RendererError> {
        let cached = self.external_imports.get(id).and_then(|imports| {
            imports
                .iter()
                .find(|imported| imported.slot == slot.index)
                .filter(|imported| imported.resource_id == slot.resource_id)
                .filter(|imported| {
                    (imported.texture.width(), imported.texture.height()) == slot.size
                })
        });
        if let Some(imported) = cached {
            return Ok(imported.clone());
        }
        let texture = import(&self.device).map_err(RendererError::Texture)?;
        let imported = ImportedTexture {
            slot: slot.index,
            resource_id: slot.resource_id,
            bind_group: self.image_bind_group(&texture),
            texture,
        };
        let imports = self.external_imports.entry(id.clone()).or_default();
        imports.retain(|existing| existing.slot != slot.index);
        imports.push(imported.clone());
        Ok(imported)
    }

    /// Drops the imports of producer textures that were retired. A frame
    /// still on screen keeps its texture until it is replaced.
    pub(crate) fn forget_external_resources(&mut self, resource_ids: &[u64]) {
        for imports in self.external_imports.values_mut() {
            imports.retain(|imported| !resource_ids.contains(&imported.resource_id));
        }
    }

    pub(in crate::render::gpu) fn retire_external_texture(&mut self, id: &ImageId) {
        if let Some(release) = self.external_texture_releases.remove(id) {
            self.queue.on_submitted_work_done(move || drop(release));
            self.submission_poller.notify();
        }
    }

    #[cfg(windows)]
    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }
}
