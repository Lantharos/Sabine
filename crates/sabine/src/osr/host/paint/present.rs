use sabine_platform::request_window_effect;

use crate::osr::protocol::{MAIN_TEXTURE_ID, POPUP_OVERLAY_ID};
#[cfg(target_os = "linux")]
use crate::render::DisplayCommand;
use crate::render::{DisplayList, ImageCommand, RectCommand, RoundedRectCommand};
use crate::window::style::Color;

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{SurfaceGeometry, overlay_texture_id, uses_sabine_chrome};

impl OsrNativeHost {
    pub(in crate::osr::host) fn present_rendered_surface(&mut self, trace: &str) {
        if self.presented {
            return;
        }
        let Some(window) = self.window.clone() else {
            return;
        };
        self.presented = true;
        crate::osr::host::trace_host(&self.config, trace);
        // Drop any prior effect before binding a new one; Wayland allows only one
        // `ext_background_effect` resource per surface.
        self.effect = None;
        self.effect = request_window_effect(&window, &self.window_options());
        self.update_effect_regions();
        if self.config.visible {
            window.set_visible(true);
            window.set_minimized(false);
            if self.config.active || self.focused {
                crate::osr::host::native::present_window(&window);
            }
            window.request_redraw();
        }
    }

    pub(in crate::osr::host) fn render(&mut self) -> bool {
        let scale = self
            .window
            .as_ref()
            .map_or(1.0, |window| window.scale_factor()) as f32;
        let width = self.surface_size.width as f32 / scale.max(1.0);
        let height = self.surface_size.height as f32 / scale.max(1.0);
        let list = self.display_list(width.max(1.0), height.max(1.0));
        let Some(renderer) = self.renderer.as_mut() else {
            return false;
        };
        renderer.resize(self.surface_size.width, self.surface_size.height, scale);
        if let Err(error) = renderer.render(&list) {
            eprintln!("Sabine OSR render failed: {error}");
            return false;
        }
        if !self.main_frame_presented && self.main_surface_ready() && self.loading.is_none() {
            self.main_frame_presented = true;
            crate::osr::host::trace_host(&self.config, "browser.first_paint");
        }
        if self.effect_regions_dirty {
            self.effect_regions_dirty = false;
            self.update_effect_regions();
        }
        true
    }

    fn display_list(&self, width: f32, height: f32) -> DisplayList {
        let opaque_swapchain = self
            .renderer
            .as_ref()
            .is_some_and(|renderer| renderer.surface_alpha_is_opaque());
        let background = if self.config.transparent && !opaque_swapchain {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        } else {
            self.config.background_color
        };
        let mut list = DisplayList::new(background);
        if !self.config.transparent || uses_sabine_chrome(self.config.chrome) {
            let radius = if self.config.chrome.uses_native_decorations() {
                0.0
            } else {
                12.0
            };
            list.push(RoundedRectCommand {
                x: 0.0,
                y: 0.0,
                width,
                height,
                radius,
                color: self
                    .config
                    .background_color
                    .opacity(if self.config.transparent { 0.38 } else { 1.0 }),
            });
        }
        // Solid underlay for opaque regions so glass windows only show blur
        // through the sidebar (or other non-opaque areas), not the content pane.
        if self.config.transparent
            && let Some(opaque) = &self.config.regions.opaque
        {
            let region_width = width.round().max(1.0) as i32;
            let region_height = height.round().max(1.0) as i32;
            for rect in opaque.resolved_rects(region_width, region_height) {
                list.push(RectCommand {
                    x: rect.x as f32,
                    y: rect.y as f32,
                    width: rect.width as f32,
                    height: rect.height as f32,
                    color: self.config.background_color,
                });
            }
        }
        #[cfg(target_os = "linux")]
        for hole in self.media.holes() {
            list.push(DisplayCommand::Cutout(RoundedRectCommand {
                x: hole.bounds.x as f32,
                y: hole.bounds.y as f32,
                width: hole.bounds.width as f32,
                height: hole.bounds.height as f32,
                radius: hole.radius as f32,
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
            }));
        }
        self.draw_titlebar(&mut list, width);
        if self.loading.is_some_and(|loading| loading.revealed()) {
            self.draw_loading(&mut list, width, height);
            return list;
        }
        if !self.main_surface_ready() {
            return list;
        }
        let top = self.titlebar_height();
        if let Some(surface) = self.main_surface {
            list.push(surface_image(MAIN_TEXTURE_ID.to_string(), surface, top));
        }
        let popup = self.overlays.get(POPUP_OVERLAY_ID);
        for (overlay_id, overlay) in &self.overlays {
            if overlay_id.as_str() != POPUP_OVERLAY_ID {
                list.push(surface_image(
                    overlay_texture_id(overlay_id),
                    overlay.geometry,
                    top,
                ));
            }
        }
        if let Some(popup) = popup {
            list.push(surface_image(
                overlay_texture_id(POPUP_OVERLAY_ID),
                popup.geometry,
                top,
            ));
        }
        self.draw_tooltip(&mut list, width, height);
        list
    }
}

fn surface_image(id: String, surface: SurfaceGeometry, top: f32) -> ImageCommand {
    ImageCommand {
        id,
        x: surface.x as f32,
        y: top + surface.y as f32,
        width: surface.width as f32,
        height: surface.height as f32,
    }
}
