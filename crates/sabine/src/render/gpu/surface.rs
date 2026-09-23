use super::{GpuRenderer, RendererError};

pub(super) fn select_surface_alpha_mode(
    modes: &[wgpu::CompositeAlphaMode],
    transparent: bool,
) -> wgpu::CompositeAlphaMode {
    if transparent {
        for preferred in [
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
        ] {
            if modes.contains(&preferred) {
                return preferred;
            }
        }
    }
    modes
        .iter()
        .copied()
        .find(|mode| *mode == wgpu::CompositeAlphaMode::Opaque)
        .unwrap_or(modes[0])
}

impl GpuRenderer {
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f32) {
        if self.health.failure().is_some() {
            return;
        }
        if width == 0 || height == 0 {
            return;
        }
        let scale_factor = scale_factor.max(0.25);
        // Wayland often emits a configure after interactive move with the same
        // size. Reconfiguring the swapchain there flashes a blank frame.
        if self.surface_config.width == width
            && self.surface_config.height == height
            && (self.scale_factor - scale_factor).abs() < f32::EPSILON
        {
            return;
        }

        self.surface_config.width = width;
        self.surface_config.height = height;
        self.scale_factor = scale_factor;
        self.surface.configure(&self.device, &self.surface_config);
    }

    pub(super) fn acquire_surface_texture(
        &mut self,
    ) -> Result<Option<wgpu::SurfaceTexture>, RendererError> {
        // After interactive move Wayland often returns Outdated. Reconfigure and
        // retry in-place — returning without a present flashes transparent glass.
        for attempt in 0..3 {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame) => return Ok(Some(frame)),
                wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                    // Still present this buffer; refresh config for the next frame.
                    self.surface.configure(&self.device, &self.surface_config);
                    return Ok(Some(frame));
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    self.window.request_redraw();
                    return Ok(None);
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    self.surface.configure(&self.device, &self.surface_config);
                    if attempt + 1 == 3 {
                        self.window.request_redraw();
                        return Ok(None);
                    }
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    self.surface = self
                        .instance
                        .create_surface(self.window.clone())
                        .map_err(|error| RendererError::Surface(error.to_string()))?;
                    self.surface.configure(&self.device, &self.surface_config);
                    if attempt + 1 == 3 {
                        self.window.request_redraw();
                        return Ok(None);
                    }
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(RendererError::SurfaceValidation);
                }
            }
        }
        Ok(None)
    }
}
