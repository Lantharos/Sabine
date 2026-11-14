#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "windows")]
mod windows;

use std::sync::Arc;

use winit::window::Window;

#[cfg(target_os = "macos")]
use macos::Backend;
#[cfg(target_os = "linux")]
use wayland::Backend;
#[cfg(target_os = "windows")]
use windows::Backend;

use crate::{BackdropMaterial, WindowOptions, WindowRegionRect};

/// A window's background material together with its blur, opaque and input
/// regions. Regions are given in logical pixels and follow the window size.
pub struct WindowEffect {
    backend: Backend,
}

impl WindowEffect {
    /// `wake` runs when the material of the window's glass changes.
    pub fn new(
        window: &Arc<dyn Window>,
        options: &WindowOptions,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<Self> {
        #[cfg(target_os = "linux")]
        let backend = Backend::new(window, options, wake);
        #[cfg(not(target_os = "linux"))]
        let backend = {
            drop(wake);
            Backend::new(window, options)
        };
        backend.map(|backend| Self { backend })
    }

    /// The material the window's glass has.
    pub fn material(&self) -> BackdropMaterial {
        #[cfg(target_os = "linux")]
        return self.backend.material();
        #[cfg(not(target_os = "linux"))]
        BackdropMaterial::Luca
    }

    /// Reapplies the regions; `transparent_holes` stay out of the blur and
    /// opaque regions so surfaces beneath the window show there.
    pub fn update(
        &mut self,
        options: &WindowOptions,
        width: i32,
        height: i32,
        transparent_holes: &[WindowRegionRect],
    ) {
        self.backend
            .update(options, width, height, transparent_holes);
    }

    /// Retints the material for the system's light or dark appearance.
    #[cfg(target_os = "windows")]
    pub fn theme_changed(&mut self, theme: winit::window::Theme) {
        self.backend.theme_changed(theme);
    }
}
