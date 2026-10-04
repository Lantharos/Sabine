// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// On Windows, Chromium's ANGLE device must match the compositor's adapter LUID.
// A software compositor needs d3d11-warp, not ordinary d3d11. Picking a different
// device can kill shared-texture import before the first browser frame.

use std::time::Instant;

#[cfg(not(windows))]
use winit::event_loop::ActiveEventLoop;

use super::OsrNativeHost;
use crate::osr;
use crate::osr::host::socket::start_socket_reader;

impl OsrNativeHost {
    pub(in crate::osr::host) fn launch_child(&mut self) {
        if self.failure.is_some()
            || self.closing_deadline.is_some()
            || self.socket.is_some()
            || self.awaiting_connection
        {
            return;
        }
        let Some(app_id) = self
            .config
            .app_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            self.fail("Sabine OSR host requires a non-empty app_id".to_string());
            return;
        };
        let (endpoint, listener) = match crate::osr::transport::IpcEndpoint::bind(app_id) {
            Ok(connection) => connection,
            Err(error) => {
                self.fail(format!("Could not bind OSR transport: {error}"));
                return;
            }
        };
        let authentication_token = match crate::osr::transport::authentication_token() {
            Ok(token) => token,
            Err(error) => {
                self.fail(format!("Could not secure OSR transport: {error}"));
                return;
            }
        };
        #[cfg(target_os = "macos")]
        let surface_broker = match crate::osr::accel::SurfaceBroker::start(&authentication_token) {
            Ok(broker) => broker,
            Err(error) => {
                self.fail(format!("Could not share browser surfaces: {error}"));
                return;
            }
        };
        self.socket_reader = None;
        self.connection_generation = self.connection_generation.wrapping_add(1);
        let generation = self.connection_generation;
        self.awaiting_connection = true;
        self.connection_deadline = Some(Instant::now() + std::time::Duration::from_secs(30));
        self.main_load_ready = false;
        self.cef_handed_off = false;
        self.handoff_deadline = None;

        let (width, height, scale) = self.content_size_for_cef();
        let mut command = match osr::cef_osr_command(
            &self.config.runtime_dir,
            &self.config.host_binary,
            &endpoint,
            &authentication_token,
            &self.config,
            osr::CefViewport {
                width,
                height,
                scale,
                frame_rate: self.active_frame_rate(),
                parent_window: self.window.as_ref().and_then(|window| {
                    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                    match window.window_handle().ok()?.as_raw() {
                        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as u64),
                        _ => None,
                    }
                }),
                accelerated_paint: cfg!(target_os = "macos")
                    || (cfg!(windows) && self.renderer.is_some()),
            },
        ) {
            Ok(command) => command,
            Err(error) => {
                self.awaiting_connection = false;
                endpoint.unlink();
                self.fail(format!("Could not prepare the browser: {error}"));
                return;
            }
        };
        #[cfg(target_os = "macos")]
        command.arg(format!(
            "--sabine-surface-service={}",
            surface_broker.service_name()
        ));
        #[cfg(windows)]
        if let Some(renderer) = &self.renderer {
            let luid = crate::osr::accel::adapter_luid(renderer);
            let angle = if renderer.uses_software_adapter() {
                "d3d11-warp"
            } else {
                "d3d11"
            };
            command.arg(format!("--use-angle={angle}"));
            command.arg(format!("--use-adapter-luid={luid}"));
            if crate::launch::metrics::trace_enabled() {
                eprintln!("Sabine GPU: Chromium adapter LUID={luid} ANGLE={angle}");
            }
        }
        let child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.awaiting_connection = false;
                endpoint.unlink();
                self.fail(format!("Could not launch the browser: {error}"));
                return;
            }
        };
        self.socket_reader = Some(start_socket_reader(
            generation,
            listener,
            endpoint,
            authentication_token,
            self.sender.clone(),
            self.proxy.clone(),
            #[cfg(target_os = "macos")]
            surface_broker.registry(),
        ));
        #[cfg(target_os = "macos")]
        {
            self.surface_broker = Some(surface_broker);
        }
        self.children.push((generation, child));
    }

    #[cfg(not(windows))]
    pub(in crate::osr::host) fn launch_child_before_window(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
    ) {
        self.scale_factor = crate::launch::launch_scale_factor(event_loop);
        self.surface_size = winit::dpi::PhysicalSize::new(
            (f64::from(self.config.width) * self.scale_factor).round() as u32,
            (f64::from(self.config.height) * self.scale_factor).round() as u32,
        );
        self.launch_child();
    }
}
