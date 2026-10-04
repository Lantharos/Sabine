// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// DWM composes a window's material behind its whole client area, so the page's
// own transparency decides where it shows. Regions shape what Windows lets a
// window do: an empty blur region removes the material and the input region
// becomes the window's shape. Windows with system decorations keep their frame
// and are never shaped.

mod material;
mod shape;

use std::sync::Arc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HWND, RECT};
use winit::window::{Theme, Window};

use crate::{WindowBackgroundEffect, WindowOptions, WindowRegionRect};

pub(super) struct Backend {
    window: Arc<dyn Window>,
    hwnd: HWND,
    effect: WindowBackgroundEffect,
    material_shown: bool,
    shape: Option<Vec<RECT>>,
}

impl Backend {
    pub(super) fn new(window: &Arc<dyn Window>, options: &WindowOptions) -> Option<Self> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return None;
        };
        let backend = Self {
            window: Arc::clone(window),
            hwnd: HWND(handle.hwnd.get() as *mut std::ffi::c_void),
            effect: if options.wants_background_effect() {
                options.background_effect
            } else {
                WindowBackgroundEffect::None
            },
            material_shown: true,
            shape: None,
        };
        backend.show_material(window.theme().unwrap_or(Theme::Light));
        Some(backend)
    }

    pub(super) fn update(
        &mut self,
        options: &WindowOptions,
        width: i32,
        height: i32,
        _transparent_holes: &[WindowRegionRect],
    ) {
        let material_shown = options
            .regions
            .blur
            .as_ref()
            .is_none_or(|blur| !blur.is_empty());
        if material_shown != self.material_shown {
            self.material_shown = material_shown;
            if material_shown {
                self.show_material(self.window.theme().unwrap_or(Theme::Light));
            } else {
                material::hide(self.hwnd, self.effect);
            }
        }
        if options.chrome.uses_native_decorations() {
            return;
        }
        let shape = options
            .regions
            .input
            .as_ref()
            .map(|input| {
                shape::physical_rects(
                    &input.resolved_rects(width, height),
                    self.window.scale_factor(),
                )
            })
            .filter(|rects| !rects.is_empty());
        if shape != self.shape {
            shape::set_window_shape(self.hwnd, shape.as_deref());
            self.shape = shape;
        }
    }

    pub(super) fn theme_changed(&mut self, theme: Theme) {
        if self.material_shown {
            self.show_material(theme);
        }
    }

    fn show_material(&self, theme: Theme) {
        if !material::show(self.hwnd, self.effect, theme == Theme::Dark)
            && self.effect != WindowBackgroundEffect::None
            && std::env::var_os("SABINE_TRACE").is_some()
        {
            eprintln!("Sabine window effect {:?} was not applied", self.effect);
        }
    }
}
