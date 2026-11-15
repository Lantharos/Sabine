use super::{GpuRenderer, RendererError};
use crate::render::effective_scale;

pub(super) fn select_surface_alpha_mode(
    modes: &[wgpu::CompositeAlphaMode],
    transparent: bool,
) -> Result<wgpu::CompositeAlphaMode, RendererError> {
    let preferred: &[wgpu::CompositeAlphaMode] = if transparent {
        &[
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::Inherit,
            wgpu::CompositeAlphaMode::Opaque,
        ]
    } else {
        &[wgpu::CompositeAlphaMode::Opaque]
    };
    preferred_or_first(modes, preferred, "alpha modes")
}

/// Presenting never blocks the window thread: Wayland and DirectComposition
/// show the newest frame from a mailbox, and a windowed Metal layer presents
/// without waiting for the display while the window server composites it.
/// Fullscreen Metal layers can scan out directly, so they wait for vsync to
/// avoid tearing.
pub(super) fn select_present_mode(
    modes: &[wgpu::PresentMode],
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))] fullscreen: bool,
) -> Result<wgpu::PresentMode, RendererError> {
    #[cfg(target_os = "macos")]
    let preferred: &[wgpu::PresentMode] = if fullscreen {
        &[wgpu::PresentMode::Fifo]
    } else {
        &[wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo]
    };
    #[cfg(not(target_os = "macos"))]
    let preferred = &[wgpu::PresentMode::Mailbox, wgpu::PresentMode::Fifo];
    preferred_or_first(modes, preferred, "present modes")
}

/// The first of `preferred` the surface supports, or else the first it
/// supports at all.
fn preferred_or_first<T: Copy + PartialEq>(
    supported: &[T],
    preferred: &[T],
    kind: &str,
) -> Result<T, RendererError> {
    preferred
        .iter()
        .copied()
        .find(|mode| supported.contains(mode))
        .or_else(|| supported.first().copied())
        .ok_or_else(|| RendererError::Surface(format!("the surface offers no {kind}")))
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
        self.configure_surface();
    }

    fn configure_surface(&mut self) {
        self.surface_suboptimal = false;
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// The texture to draw the next frame into, or none when no frame can
    /// show now. A window the system hides draws again once it shows.
    pub(super) fn acquire_surface_texture(
        &mut self,
    ) -> Result<Option<wgpu::SurfaceTexture>, RendererError> {
        if self.surface_suboptimal {
            self.configure_surface();
        }
        for attempt in 0..3 {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame) => return Ok(Some(frame)),
                wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                    self.surface_suboptimal = true;
                    return Ok(Some(frame));
                }
                wgpu::CurrentSurfaceTexture::Occluded => return Ok(None),
                wgpu::CurrentSurfaceTexture::Timeout => {
                    self.window.request_redraw();
                    return Ok(None);
                }
                wgpu::CurrentSurfaceTexture::Outdated => {
                    self.configure_surface();
                    if attempt + 1 == 3 {
                        self.window.request_redraw();
                        return Ok(None);
                    }
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    self.surface = self.source.create(&self.instance)?;
                    self.configure_surface();
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
        let Ok(present_mode) = select_present_mode(&self.present_modes, fullscreen) else {
            return;
        };
        if present_mode != self.surface_config.present_mode {
            self.surface_config.present_mode = present_mode;
            self.configure_surface();
        }
    }
}
