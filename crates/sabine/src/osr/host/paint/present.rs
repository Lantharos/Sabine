use sabine_platform::WindowEffect;

use std::sync::Arc;

use crate::osr::protocol::POPUP_OVERLAY_ID;
use crate::render::{
    DisplayCommand, DisplayList, ImageCommand, ImageId, RectCommand, RoundedRectCommand,
};
use crate::window::style::Color;

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{SurfaceGeometry, uses_sabine_chrome};

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
        self.effect = WindowEffect::new(&window, &self.window_options());
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
        let scale = self.scale() as f32;
        let mut list = std::mem::take(&mut self.display_list);
        self.fill_display_list(
            &mut list,
            self.logical_width().max(1.0),
            self.logical_height().max(1.0),
        );
        let rendered = self.renderer.as_mut().map(|renderer| {
            renderer.resize(self.surface_size.width, self.surface_size.height, scale);
            renderer.render(&list)
        });
        self.display_list = list;
        let Some(rendered) = rendered else {
            return false;
        };
        if let Err(error) = rendered {
            sabine_runtime::report_error("render", format!("drawing the window failed: {error}"));
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

    fn fill_display_list(&self, list: &mut DisplayList, width: f32, height: f32) {
        let opaque_swapchain = self
            .renderer
            .as_ref()
            .is_some_and(|renderer| renderer.surface_alpha_is_opaque());
        let background = if self.config.transparent && !opaque_swapchain {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        } else {
            self.config.background_color
        };
        list.reset(background);
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
        self.draw_titlebar(list, width);
        if self.loading.is_some_and(|loading| loading.revealed()) {
            self.draw_loading(list, width, height);
            return;
        }
        if !self.main_surface_ready() {
            return;
        }
        let top = self.titlebar_height();
        if let Some(surface) = self.main_surface {
            list.push(surface_image(ImageId::Main, surface, top));
        }
        let popup = self.overlays.get_key_value(POPUP_OVERLAY_ID);
        for (overlay_id, overlay) in &self.overlays {
            if &**overlay_id != POPUP_OVERLAY_ID {
                let image = ImageId::Overlay(Arc::clone(overlay_id));
                list.push(surface_image(image, overlay.geometry, top));
            }
        }
        if let Some((popup_id, popup)) = popup {
            let image = ImageId::Overlay(Arc::clone(popup_id));
            list.push(surface_image(image, popup.geometry, top));
        }
        self.draw_tooltip(list, width, height);
        self.draw_context_menu(list);
    }
}

fn surface_image(id: ImageId, surface: SurfaceGeometry, top: f32) -> ImageCommand {
    ImageCommand {
        id,
        x: surface.x as f32,
        y: top + surface.y as f32,
        width: surface.width as f32,
        height: surface.height as f32,
    }
}
