use super::{GpuRenderer, RendererError};
use crate::render::effective_scale;

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

/// Presenting never blocks the window thread: Wayland and DirectComposition
/// show the newest frame from a mailbox, and a windowed Metal layer presents
/// without waiting for the display while the window server composites it.
/// Fullscreen Metal layers can scan out directly, so they wait for vsync to
/// avoid tearing.
pub(super) fn select_present_mode(
    modes: &[wgpu::PresentMode],
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))] fullscreen: bool,
) -> wgpu::PresentMode {
    #[cfg(target_os = "macos")]
    let preferred: &[wgpu::PresentMode] = if fullscreen {
        &[wgpu::PresentMode::Fifo]
    } else {
        &[wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo]
    };
    #[cfg(not(target_os = "macos"))]
    let preferred = &[wgpu::PresentMode::Mailbox, wgpu::PresentMode::Fifo];
    preferred
        .iter()
        .copied()
        .find(|mode| modes.contains(mode))
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
        let scale_factor = effective_scale(f64::from(scale_factor)) as f32;
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
        for attempt in 0..3 {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame) => return Ok(Some(frame)),
                wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
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
                    self.surface = self.source.create(&self.instance)?;
                    self.surface.configure(&self.device, &self.surface_config);
                    #[cfg(windows)]
                    self.source.commit();
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

#[cfg(target_os = "macos")]
impl GpuRenderer {
    pub(crate) fn set_fullscreen(&mut self, fullscreen: bool) {
        let present_mode = select_present_mode(&self.present_modes, fullscreen);
        if present_mode != self.surface_config.present_mode {
            self.surface_config.present_mode = present_mode;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }
}
