use std::sync::Arc;

#[cfg(target_os = "linux")]
use winit::platform::wayland::WindowAttributesWayland;
#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesWindows;
use winit::{
    dpi::LogicalSize,
    event_loop::ActiveEventLoop,
    window::{Window as WinitWindow, WindowAttributes, WindowLevel},
};

use crate::render::GpuRenderer;

use super::OsrNativeHost;

impl OsrNativeHost {
    pub(in crate::osr::host) fn ensure_window(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_none() {
            self.create_window(event_loop);
        }
    }

    pub(in crate::osr::host) fn create_window(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let activating = self.pending_activation_token.is_some();
        let defer_visibility = self.config.visible && !activating;
        let mut attributes = WindowAttributes::default()
            .with_title(&*self.config.title)
            .with_surface_size(LogicalSize::new(
                f64::from(self.config.width),
                f64::from(self.config.height),
            ))
            .with_min_surface_size(LogicalSize::new(
                f64::from(self.config.min_width),
                f64::from(self.config.min_height),
            ))
            .with_resizable(self.config.resizable)
            .with_decorations(self.config.chrome.uses_native_decorations())
            .with_visible(self.config.visible && !defer_visibility)
            .with_active((self.config.active || activating) && !defer_visibility)
            .with_window_level(if self.config.always_on_top {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            })
            .with_transparent(self.config.transparent);
        // On Linux, Sabine applies `ext_background_effect_v1` itself (with
        // blur/opaque/input regions). winit's `with_blur(true)` also creates an
        // effect on the same surface, and a second bind is a protocol error that
        // kills the Wayland connection.
        #[cfg(target_os = "linux")]
        {
            let mut wayland_attributes = WindowAttributesWayland::default();
            if let Some(app_id) = &self.config.app_id {
                wayland_attributes = wayland_attributes.with_name(app_id, app_id);
            }
            if let Some(token) = self.pending_activation_token.take() {
                wayland_attributes = wayland_attributes.with_activation_token(token);
            }
            attributes = attributes.with_platform_attributes(Box::new(wayland_attributes));
        }
        #[cfg(target_os = "windows")]
        if self.config.transparent || self.config.skip_taskbar {
            let windows_attributes = WindowAttributesWindows::default()
                .with_no_redirection_bitmap(self.config.transparent)
                .with_skip_taskbar(self.config.skip_taskbar);
            attributes = attributes.with_platform_attributes(Box::new(windows_attributes));
        }
        if let Some(position) =
            crate::centered_window_position(event_loop, self.config.width, self.config.height)
        {
            attributes = attributes.with_position(position);
        }
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::<dyn WinitWindow>::from(window),
            Err(error) => {
                self.fail(format!("Could not create the application window: {error}"));
                event_loop.exit();
                return;
            }
        };
        self.surface_size = window.surface_size();
        self.scale_factor = window.scale_factor();
        self.occluded = false;
        let proxy = self.proxy.clone();
        let renderer = match pollster::block_on(GpuRenderer::new(
            window.clone(),
            self.config.transparent,
            move || proxy.wake_up(),
        )) {
            Ok(renderer) => renderer,
            Err(error) => {
                self.fail(format!("Could not initialize GPU rendering: {error}"));
                event_loop.exit();
                return;
            }
        };
        self.renderer = Some(renderer);
        self.window = Some(window.clone());
        if let Err(error) = self.media.attach(window.as_ref(), self.media_viewport()) {
            eprintln!("Sabine media: {error}");
        }
        #[cfg(target_os = "linux")]
        self.attach_clipboard(window.as_ref());
        self.restore_ime_state();
        #[cfg(not(target_os = "macos"))]
        if let Err(error) = self.restore_shortcut_inhibitor() {
            eprintln!("Sabine could not inhibit desktop shortcuts: {error}");
        }
        self.send_screen_origin();
        self.launch_child();
        if let Err(error) = self.restore_retained_frames() {
            self.fail(format!("Could not restore window textures: {error}"));
            return;
        }
        if self.main_surface.is_some() {
            self.present_rendered_surface("first_paint");
        } else {
            self.send_control("repaint\n");
        }
        if self.config.visible
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }

    pub(in crate::osr::host) fn drop_hidden_window(&mut self) {
        self.main_surface = None;
        self.overlays.clear();
        self.pending_resize_paint = None;
        self.drop_presented_window();
    }

    pub(in crate::osr::host) fn unmap_window(&mut self) {
        #[cfg(target_os = "linux")]
        self.drop_presented_window();

        #[cfg(not(target_os = "linux"))]
        if let Some(window) = &self.window {
            window.set_visible(false);
        }
    }

    pub(in crate::osr::host) fn drop_presented_window(&mut self) {
        if let Some(window) = &self.window {
            let scale = crate::render::effective_scale(window.scale_factor());
            self.config.width = (f64::from(self.surface_size.width) / scale)
                .round()
                .max(f64::from(self.config.min_width)) as u32;
            self.config.height = (f64::from(self.surface_size.height) / scale)
                .round()
                .max(f64::from(self.config.min_height)) as u32;
        }
        self.media.detach();
        #[cfg(not(target_os = "macos"))]
        self.release_shortcut_inhibitor();
        self.retain_frames();
        self.window = None;
        self.renderer = None;
        self.effect = None;
        self.presented = false;
        self.hovered_control = None;
        self.pressed_control = None;
        self.cursor.forget_window();
        self.forward_ime(winit::event::Ime::Disabled);
    }

    pub(in crate::osr::host) fn set_regions(&mut self, regions: sabine_platform::WindowRegions) {
        self.config.regions = regions;
        let Some(window) = self.window.clone().filter(|_| self.presented) else {
            return;
        };
        if self.effect.is_none() {
            self.effect = sabine_platform::request_window_effect(&window, &self.window_options());
        }
        self.update_effect_regions();
        window.request_redraw();
    }

    pub(in crate::osr::host) fn update_effect_regions(&self) {
        let Some(effect) = &self.effect else {
            return;
        };
        let width = self.logical_width().round().max(1.0) as i32;
        let height = self.logical_height().round().max(1.0) as i32;
        let holes = self
            .media
            .holes()
            .map(|hole| {
                let surface = hole.surface;
                sabine_platform::WindowRegionRect::new(
                    surface.x,
                    surface.y,
                    surface.width,
                    surface.height,
                )
            })
            .collect::<Vec<_>>();
        let _ = effect.update(&self.window_options(), width, height, &holes);
    }
}
