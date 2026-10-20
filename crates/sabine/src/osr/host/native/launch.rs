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
#[cfg(target_os = "linux")]
use crate::osr::host::types::OsrHostEvent;

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

        let page = self.page_viewport();
        let accelerated_paint = self.accelerated_paint();
        #[cfg(target_os = "linux")]
        {
            self.accelerated_launch = accelerated_paint;
        }
        let launch = match osr::browser_launch(
            &self.config.runtime_dir,
            &self.config.host_binary,
            &endpoint,
            &authentication_token,
            &self.config,
            osr::CefViewport {
                page,
                frame_rate: self.active_frame_rate(),
                parent_window: self.window.as_ref().and_then(|window| {
                    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                    match window.window_handle().ok()?.as_raw() {
                        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as u64),
                        _ => None,
                    }
                }),
                accelerated_paint,
            },
        ) {
            Ok(launch) => launch,
            Err(error) => {
                self.awaiting_connection = false;
                endpoint.unlink();
                self.fail(format!("Could not prepare the browser: {error}"));
                return;
            }
        };
        #[cfg_attr(target_os = "linux", allow(unused_mut))]
        let mut command = launch.own;
        #[cfg(target_os = "macos")]
        command.arg(format!(
            "--sabine-surface-service={}",
            surface_broker.service_name()
        ));
        #[cfg(windows)]
        {
            self.browser_adapter = self.renderer.as_ref().map(|renderer| {
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
                luid
            });
        }
        #[cfg(target_os = "linux")]
        let spawned = match launch.shared.filter(|_| !self.shared_runtime_refused) {
            Some(shared) => {
                self.open_in_shared_runtime(generation, shared, command);
                Ok(None)
            }
            None => command.spawn().map(Some),
        };
        #[cfg(not(target_os = "linux"))]
        let spawned = command.spawn().map(Some);
        let child = match spawned {
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
        if let Some(child) = child {
            self.children.push((generation, child));
        }
    }

    /// Asks the shared runtime for the window's browser off the event loop,
    /// keeping the app's own Chromium ready in case the runtime cannot serve
    /// the app.
    #[cfg(target_os = "linux")]
    fn open_in_shared_runtime(
        &mut self,
        generation: u64,
        shared: crate::launch::shared::SharedLaunch,
        own: std::process::Command,
    ) {
        self.own_browser = Some((generation, own));
        let sender = self.sender.clone();
        let proxy = self.proxy.clone();
        let opening = std::thread::Builder::new()
            .name("sabine-runtime-open".to_string())
            .spawn(move || {
                let event = OsrHostEvent::SharedRuntimeOpened(generation, shared.open());
                if sender.send(event).is_ok() {
                    proxy.wake_up();
                }
            });
        if let Err(error) = opening {
            self.shared_runtime_opened(generation, Err(error.to_string()));
        }
    }

    /// Starts the app's own Chromium when the shared runtime could not open
    /// the window, and keeps doing so for this window's later browsers.
    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn shared_runtime_opened(
        &mut self,
        generation: u64,
        opened: Result<(), String>,
    ) {
        let Some((own_generation, mut command)) = self.own_browser.take() else {
            return;
        };
        let Err(reason) = opened else {
            crate::osr::host::trace_host(&self.config, "browser.shared");
            return;
        };
        if own_generation != generation {
            return;
        }
        sabine_runtime::report_error(
            "window",
            format!(
                "the shared runtime could not open this window, so the app starts its own: {reason}"
            ),
        );
        self.shared_runtime_refused = true;
        match command.spawn() {
            Ok(child) => self.children.push((generation, child)),
            Err(error) => self.fail(format!("Could not launch the browser: {error}")),
        }
    }

    /// Closes the browser and starts it again, so it picks up how the window
    /// paints now.
    #[cfg(any(windows, target_os = "linux"))]
    pub(in crate::osr::host) fn relaunch_browser(&mut self) {
        if self.socket.is_none() && !self.awaiting_connection {
            return;
        }
        self.send_control("close\n");
        self.drop_connection();
        self.awaiting_connection = false;
        self.connection_deadline = None;
        self.main_surface = None;
        self.overlays.clear();
        if let Some(renderer) = &mut self.renderer {
            renderer.clear_images();
        }
        if self.config.visible {
            self.start_loading();
        }
        self.launch_child();
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
