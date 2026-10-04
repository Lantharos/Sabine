// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The Windows shared-texture path must keep its release callback until GPU
// sampling finishes. Its crop UVs describe pixel edges, not pixel centers;
// adding a half-texel inset here resamples the whole page into a blurry mess.

use super::{CachedTexture, GpuRenderer, RendererError};
use crate::render::ImageId;

/// The producer slot an external frame was copied into, and the texture that
/// slot currently holds.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ExternalSlot {
    pub index: u32,
    pub resource_id: u64,
}

#[derive(Clone)]
pub(super) struct ImportedTexture {
    slot: u32,
    resource_id: u64,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

impl GpuRenderer {
    /// Show a frame from a producer-owned texture. Each slot's texture is
    /// imported once and reused until the producer replaces it.
    pub(crate) fn set_external_bgra_texture(
        &mut self,
        id: &ImageId,
        slot: ExternalSlot,
        import: impl FnOnce(&wgpu::Device) -> Result<wgpu::Texture, String>,
        source_origin: (u32, u32),
        size: (u32, u32),
        completed: impl FnOnce() + Send + 'static,
    ) -> Result<(), RendererError> {
        let imported = match self
            .check_device()
            .and_then(|()| self.imported_texture(id, slot, import))
        {
            Ok(imported) => imported,
            Err(error) => {
                completed();
                return Err(error);
            }
        };
        let (width, height) = size;
        if width == 0 || height == 0 {
            completed();
            return Err(RendererError::Texture(
                "external image has empty size".to_string(),
            ));
        }
        let source_width = imported.texture.width();
        let source_height = imported.texture.height();
        if source_origin.0.saturating_add(width) > source_width
            || source_origin.1.saturating_add(height) > source_height
        {
            completed();
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
        self.external_texture_releases
            .insert(id.clone(), Box::new(completed));
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

    pub(super) fn retire_external_texture(&mut self, id: &ImageId) {
        if let Some(completed) = self.external_texture_releases.remove(id) {
            self.queue.on_submitted_work_done(completed);
            self.submission_poller.notify();
        }
    }

    #[cfg(windows)]
    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }
}
