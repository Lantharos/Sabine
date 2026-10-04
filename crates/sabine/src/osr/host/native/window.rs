use std::{io::Write, sync::Arc};

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowAttributesMacOS;
#[cfg(target_os = "linux")]
use winit::platform::wayland::WindowAttributesWayland;
#[cfg(target_os = "windows")]
use winit::platform::windows::WindowAttributesWindows;
use winit::{
    dpi::LogicalSize,
    event_loop::ActiveEventLoop,
    window::{ResizeDirection, Window as WinitWindow, WindowAttributes, WindowLevel},
};

use crate::osr::host::ui::chrome::resize_direction_at;
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
        #[cfg(target_os = "linux")]
        {
            attributes = attributes.with_platform_attributes(Box::new(self.wayland_attributes()));
        }
        #[cfg(target_os = "macos")]
        if self.config.titlebar_overlay {
            attributes = attributes.with_platform_attributes(Box::new(
                WindowAttributesMacOS::default()
                    .with_titlebar_transparent(true)
                    .with_title_hidden(true)
                    .with_fullsize_content_view(true),
            ));
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
            sabine_runtime::report_error("media", error);
        }
        #[cfg(target_os = "linux")]
        self.attach_clipboard(window.as_ref());
        self.restore_ime_state();
        if let Err(error) = self.restore_shortcut_inhibitor() {
            sabine_runtime::report_error(
                "input",
                format!("could not inhibit desktop shortcuts: {error}"),
            );
        }
        self.send_screen_origin();
        self.send_controls_overlay();
        self.refresh_appearance();
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

    /// Leaves blur to Sabine's own background effect: a surface has room for
    /// one, and winit binding a second is a protocol error that ends the
    /// Wayland connection.
    #[cfg(target_os = "linux")]
    fn wayland_attributes(&mut self) -> WindowAttributesWayland {
        let mut attributes = WindowAttributesWayland::default();
        if let Some(app_id) = &self.config.app_id {
            attributes = attributes.with_name(app_id, app_id);
        }
        if let Some(token) = self.pending_activation_token.take() {
            attributes = attributes.with_activation_token(token);
        }
        attributes
    }

    /// The corner of the page the system's window controls cover.
    pub(in crate::osr::host) fn controls_overlay(&self) -> serde_json::Value {
        #[cfg(target_os = "macos")]
        if self.config.titlebar_overlay
            && let Some(overlay) = self
                .window
                .as_ref()
                .and_then(|window| sabine_platform::controls_overlay(window.as_ref()))
        {
            return serde_json::json!({ "left": overlay.left, "right": 0, "height": overlay.height });
        }
        serde_json::json!({ "left": 0, "right": 0, "height": 0 })
    }

    fn send_controls_overlay(&self) {
        self.send_control(format!(
            "SABINE_BRIDGE_EVENT\t\"{}\"\t{}\n",
            sabine_bridge::CONTROLS_OVERLAY_EVENT,
            self.controls_overlay()
        ));
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

    /// The edge Sabine resizes the window from, for windows without system
    /// decorations. AppKit resizes borderless windows from their edges itself.
    pub(in crate::osr::host) fn resize_edge_under_cursor(&self) -> Option<ResizeDirection> {
        let draws_edges = self.config.resizable
            && !self.config.chrome.uses_native_decorations()
            && !cfg!(target_os = "macos");
        draws_edges
            .then(|| {
                resize_direction_at(
                    self.cursor_x,
                    self.cursor_y,
                    self.logical_width(),
                    self.logical_height(),
                )
            })
            .flatten()
    }

    #[cfg(target_os = "windows")]
    pub(in crate::osr::host) fn retint_effect(&mut self, theme: winit::window::Theme) {
        if let Some(effect) = &mut self.effect {
            effect.theme_changed(theme);
        }
    }

    fn current_appearance(&self) -> Option<sabine_platform::Appearance> {
        let window = self.window.as_ref()?;
        #[cfg(target_os = "linux")]
        return Some(self.appearance.appearance(window.as_ref()));
        #[cfg(not(target_os = "linux"))]
        return Some(sabine_platform::system_appearance(window.as_ref()));
    }

    /// Tells the page and the app when the desktop's appearance changed.
    pub(in crate::osr::host) fn refresh_appearance(&mut self) {
        let Some(appearance) = self.current_appearance() else {
            return;
        };
        if self.published_appearance == Some(appearance) {
            return;
        }
        self.published_appearance = Some(appearance);
        self.send_control(format!(
            "SABINE_BRIDGE_EVENT\t\"{}\"\t{}\n",
            sabine_bridge::APPEARANCE_EVENT,
            appearance_json(&appearance)
        ));
        let mut output = std::io::stdout().lock();
        let _ = writeln!(output, "{}", crate::window::appearance_line(&appearance));
        let _ = output.flush();
    }

    pub(in crate::osr::host) fn appearance_json(&self) -> serde_json::Value {
        self.published_appearance
            .or_else(|| self.current_appearance())
            .map_or(serde_json::Value::Null, |appearance| {
                appearance_json(&appearance)
            })
    }

    pub(in crate::osr::host) fn set_regions(&mut self, regions: sabine_platform::WindowRegions) {
        self.config.regions = regions;
        self.update_effect_regions();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(in crate::osr::host) fn update_effect_regions(&mut self) {
        let options = self.window_options();
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
        if let Some(effect) = &mut self.effect {
            effect.update(&options, width, height, &holes);
        }
    }
}

fn appearance_json(appearance: &sabine_platform::Appearance) -> serde_json::Value {
    serde_json::json!({
        "colorScheme": if appearance.dark { "dark" } else { "light" },
        "accentColor": appearance.accent_hex(),
    })
}
