// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// On Windows, Chromium's ANGLE device must match the compositor's adapter LUID.
// A software compositor needs d3d11-warp, not ordinary d3d11. Picking a different
// device can kill shared-texture import before the first browser frame.

mod window;

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
    process::Child,
    sync::{Arc, mpsc},
    time::Instant,
};

use sabine_platform::{WindowChrome as PlatformWindowChrome, WindowOptions, WindowRegionRect};
use winit::{
    cursor::CursorIcon,
    data_transfer::DataTransferId,
    event_loop::{ActiveEventLoop, DndAction, EventLoopProxy},
    window::{ActivationToken, Window as WinitWindow},
};

use crate::bridge::frame::Frame;
use crate::osr::control::{ControlRelay, ControlWriter};
use crate::osr::frame_buffer::FrameBuffer;

use crate::osr::transport::IpcStream;
use crate::render::GpuRenderer;
use crate::{SabineWindowChrome, osr};
use sabine_platform::WindowEffect;

use super::config::OsrHostConfig;
use super::socket::{SocketReader, start_socket_reader};
use super::types::{
    ClickMemory, LifecycleState, MouseButtons, OsrHostEvent, OverlayLayer, PendingResizePaint,
    SurfaceGeometry, TitlebarControl, uses_sabine_chrome,
};

pub(super) struct OsrNativeHost {
    pub(super) config: OsrHostConfig,
    pub(super) sender: mpsc::SyncSender<OsrHostEvent>,
    pub(super) receiver: mpsc::Receiver<OsrHostEvent>,
    pub(super) proxy: EventLoopProxy,
    pub(super) window: Option<Arc<dyn WinitWindow>>,
    pub(super) renderer: Option<GpuRenderer>,
    pub(super) effect: Option<WindowEffect>,
    pub(super) children: Vec<(u64, Child)>,
    pub(super) socket: Option<IpcStream>,
    pub(super) socket_reader: Option<SocketReader>,
    pub(super) control_writer: Option<Arc<ControlWriter>>,
    pub(super) relay: ControlRelay,
    pub(super) pending_messages: Option<(u64, Arc<crate::osr::message_queue::MessageQueue>)>,
    pub(super) connection_generation: u64,
    pub(super) awaiting_connection: bool,
    pub(super) connection_deadline: Option<Instant>,
    pub(super) recovery_deadline: Option<Instant>,
    pub(super) recoveries: VecDeque<Instant>,
    pub(super) gpu_recoveries: VecDeque<Instant>,
    pub(super) failure: Option<String>,
    pub(super) surface_size: winit::dpi::PhysicalSize<u32>,
    pub(super) scale_factor: f64,
    pub(super) main_surface: Option<SurfaceGeometry>,
    pub(super) main_load_ready: bool,
    pub(super) main_buffer: FrameBuffer,
    pub(super) overlays: BTreeMap<String, OverlayLayer>,
    pub(super) page_drag_regions: Vec<WindowRegionRect>,
    pub(super) page_drag_exclusion_regions: Vec<WindowRegionRect>,
    pub(super) hovered_control: Option<TitlebarControl>,
    pub(super) pressed_control: Option<TitlebarControl>,
    pub(super) cursor: CursorIcon,
    pub(super) native_cursor_override: bool,
    pub(super) modifiers: winit::keyboard::ModifiersState,
    pub(super) mouse: MouseButtons,
    pub(super) last_click: Option<ClickMemory>,
    pub(super) active_click_count: i32,
    pub(super) cursor_x: f32,
    pub(super) cursor_y: f32,
    pub(super) focused: bool,
    pub(super) ime_mode: u32,
    pub(super) ime_cursor_area: (i32, i32, u32, u32),
    pub(super) ime_surrounding: super::types::ImeSurrounding,
    pub(super) ime_preedit: Option<super::types::ImePreedit>,
    pub(super) occluded: bool,
    pub(super) published_window_state: Option<super::types::WindowState>,
    pub(super) lifecycle_state: LifecycleState,
    pub(super) last_frame_rate: Cell<Option<u32>>,
    pub(super) hibernate_deadline: Option<Instant>,
    pub(super) hibernate_commit_deadline: Option<Instant>,
    pub(super) closing_deadline: Option<Instant>,
    pub(super) pending_resize_paint: Option<PendingResizePaint>,
    pub(super) pending_suspend_at: Option<Instant>,
    pub(super) effect_regions_dirty: bool,
    pub(super) activity_hibernation_blockers: BTreeSet<String>,
    pub(super) presented: bool,
    pub(super) main_frame_presented: bool,
    pub(super) loading: Option<super::types::NativeLoading>,
    pub(super) tooltip: Option<super::types::NativeTooltip>,
    pub(super) pending_activation_token: Option<ActivationToken>,
    pub(super) active_file_drag: Option<DataTransferId>,
    pub(super) incoming_file_drag: Option<IncomingFileDrag>,
    #[cfg(not(target_os = "macos"))]
    pub(super) shortcuts: super::input::ShortcutInhibition,
    /// CEF exited with process-singleton handoff (code 24). The existing
    /// browser process owns this window's OSR endpoint; keep listening.
    pub(super) cef_handed_off: bool,
    /// Deadline for the primary CEF process to connect after exit-24 handoff.
    pub(super) handoff_deadline: Option<Instant>,
    #[cfg(target_os = "macos")]
    pub(super) surface_broker: Option<crate::osr::accel::SurfaceBroker>,
    #[cfg(target_os = "linux")]
    pub(super) media: crate::media::MediaHost,
    #[cfg(target_os = "linux")]
    pub(super) clipboard: Option<crate::clipboard::SystemClipboard>,
    #[cfg(target_os = "linux")]
    pub(super) paste_gesture: Option<Instant>,
}

impl OsrNativeHost {
    pub(super) fn new(
        config: OsrHostConfig,
        sender: mpsc::SyncSender<OsrHostEvent>,
        receiver: mpsc::Receiver<OsrHostEvent>,
        proxy: EventLoopProxy,
    ) -> Self {
        let relay = ControlRelay::default();
        start_parent_bridge_reader(sender.clone(), proxy.clone(), relay.clone());
        let surface_size = winit::dpi::PhysicalSize::new(config.width, config.height);
        let visible = config.visible;
        let focused = visible && config.active;
        let lifecycle_state = if visible {
            LifecycleState::Active
        } else {
            LifecycleState::Suspended
        };
        let hibernate_deadline = if visible {
            None
        } else {
            config
                .lifecycle
                .hibernate_after
                .map(|delay| Instant::now() + delay)
        };
        #[cfg(target_os = "linux")]
        let media = crate::media::MediaHost::new(config.transparent);
        Self {
            config,
            sender,
            receiver,
            proxy,
            window: None,
            renderer: None,
            effect: None,
            children: Vec::new(),
            socket: None,
            socket_reader: None,
            control_writer: None,
            relay,
            pending_messages: None,
            connection_generation: 0,
            awaiting_connection: false,
            connection_deadline: None,
            recovery_deadline: None,
            recoveries: VecDeque::new(),
            gpu_recoveries: VecDeque::new(),
            failure: None,
            surface_size,
            scale_factor: 1.0,
            main_surface: None,
            main_load_ready: false,
            main_buffer: FrameBuffer::new(),
            overlays: BTreeMap::new(),
            page_drag_regions: Vec::new(),
            page_drag_exclusion_regions: Vec::new(),
            hovered_control: None,
            pressed_control: None,
            cursor: CursorIcon::Default,
            native_cursor_override: false,
            modifiers: Default::default(),
            mouse: MouseButtons::default(),
            last_click: None,
            active_click_count: 1,
            cursor_x: 0.0,
            cursor_y: 0.0,
            focused,
            ime_mode: 1,
            ime_cursor_area: (0, 0, 1, 24),
            ime_surrounding: Default::default(),
            ime_preedit: None,
            occluded: false,
            published_window_state: None,
            lifecycle_state,
            last_frame_rate: Cell::new(None),
            hibernate_deadline,
            hibernate_commit_deadline: None,
            closing_deadline: None,
            pending_resize_paint: None,
            pending_suspend_at: None,
            effect_regions_dirty: false,
            activity_hibernation_blockers: BTreeSet::new(),
            presented: false,
            main_frame_presented: false,
            loading: visible
                .then(|| super::types::NativeLoading::new(super::types::LoadingKind::Opening)),
            tooltip: None,
            pending_activation_token: None,
            active_file_drag: None,
            incoming_file_drag: None,
            #[cfg(not(target_os = "macos"))]
            shortcuts: Default::default(),
            cef_handed_off: false,
            handoff_deadline: None,
            #[cfg(target_os = "macos")]
            surface_broker: None,
            #[cfg(target_os = "linux")]
            media,
            #[cfg(target_os = "linux")]
            clipboard: None,
            #[cfg(target_os = "linux")]
            paste_gesture: None,
        }
    }

    pub(super) fn launch_child(&mut self) {
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
            if std::env::var_os("SABINE_TRACE").is_some() {
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
    pub(super) fn launch_child_before_window(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.scale_factor = crate::launch::launch_scale_factor(event_loop);
        self.surface_size = winit::dpi::PhysicalSize::new(
            (f64::from(self.config.width) * self.scale_factor).round() as u32,
            (f64::from(self.config.height) * self.scale_factor).round() as u32,
        );
        self.launch_child();
    }

    pub(super) fn content_size_for_cef(&self) -> (u32, u32, f64) {
        let scale = self
            .window
            .as_ref()
            .map_or(self.scale_factor, |window| window.scale_factor());
        if !self.config.visible
            && self.window.is_none()
            && self.config.lifecycle.hibernate_after.is_some()
        {
            return (1, 1, scale);
        }
        let logical_width = f64::from(self.surface_size.width) / scale.max(1.0);
        let logical_height = (f64::from(self.surface_size.height) / scale.max(1.0)
            - f64::from(self.titlebar_height()))
        .max(1.0);
        (
            logical_width.round().max(1.0) as u32,
            logical_height.round().max(1.0) as u32,
            scale,
        )
    }

    pub(super) fn titlebar_height(&self) -> f32 {
        if uses_sabine_chrome(self.config.chrome) {
            super::types::TITLEBAR_HEIGHT
        } else {
            0.0
        }
    }

    pub(super) fn window_options(&self) -> WindowOptions {
        WindowOptions {
            title: self.config.title.clone(),
            width: self.config.width,
            height: self.config.height,
            min_width: self.config.min_width,
            min_height: self.config.min_height,
            chrome: platform_chrome(self.config.chrome),
            resizable: self.config.resizable,
            visible: self.config.visible,
            active: self.config.active,
            always_on_top: self.config.always_on_top,
            transparent: self.config.transparent,
            background_effect: self.config.background_effect,
            regions: self.config.regions.clone(),
        }
    }

    pub(super) fn send_control(&self, line: &str) {
        let Some(writer) = &self.control_writer else {
            return;
        };
        if let Err(error) = writer.send(line.to_string()) {
            eprintln!("Sabine native OSR control send failed: {error}");
        }
    }

    pub(super) fn send_mouse_motion(&self, line: String) {
        let Some(writer) = &self.control_writer else {
            return;
        };
        if let Err(error) = writer.send_motion(line) {
            eprintln!("Sabine native OSR pointer send failed: {error}");
        }
    }

    pub(super) fn content_surface_size(&self) -> (u32, u32) {
        let (width, height, _) = self.content_size_for_cef();
        (width, height)
    }

    pub(super) fn content_position(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let titlebar_height = self.titlebar_height();
        (y >= titlebar_height).then_some((x.max(0.0), (y - titlebar_height).max(0.0)))
    }

    pub(super) fn logical_width(&self) -> f32 {
        let scale = self
            .window
            .as_ref()
            .map_or(1.0, |window| window.scale_factor()) as f32;
        self.surface_size.width as f32 / scale.max(1.0)
    }

    pub(super) fn logical_height(&self) -> f32 {
        let scale = self
            .window
            .as_ref()
            .map_or(1.0, |window| window.scale_factor()) as f32;
        self.surface_size.height as f32 / scale.max(1.0)
    }

    pub(super) fn begin_close(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.closing_deadline.is_some() {
            return;
        }
        if let Some(window) = &self.window {
            window.set_visible(false);
        }
        self.send_control("close\n");
        self.closing_deadline = Some(Instant::now() + super::types::CLOSE_GRACE);
        if self.children.is_empty() && self.socket.is_none() && !self.awaiting_connection {
            self.force_close(event_loop);
        }
    }

    pub(super) fn force_close(&mut self, event_loop: &dyn ActiveEventLoop) {
        for (_, child) in &mut self.children {
            let _ = child.try_wait();
        }
        self.drop_connection();
        event_loop.exit();
    }

    pub(super) fn drop_connection(&mut self) {
        if let Some(socket) = self.socket.take() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        self.socket_reader = None;
        self.relay.connect(None);
        self.control_writer = None;
        self.pending_messages = None;
        #[cfg(target_os = "macos")]
        {
            self.surface_broker = None;
        }
    }
}

pub(super) struct IncomingFileDrag {
    pub(super) id: Option<DataTransferId>,
    pub(super) paths: Vec<PathBuf>,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) action: Option<DndAction>,
    pub(super) entered: bool,
    pub(super) dropped: bool,
}

pub(super) fn platform_chrome(chrome: SabineWindowChrome) -> PlatformWindowChrome {
    match chrome {
        SabineWindowChrome::System => PlatformWindowChrome::System,
        SabineWindowChrome::Sabine => PlatformWindowChrome::Sabine,
        SabineWindowChrome::Frameless | SabineWindowChrome::None => PlatformWindowChrome::None,
    }
}

pub(super) fn present_window(window: &Arc<dyn WinitWindow>) {
    window.set_visible(true);
    window.set_minimized(false);
    window.focus_window();
    window.request_redraw();
}

fn start_parent_bridge_reader(
    sender: mpsc::SyncSender<OsrHostEvent>,
    proxy: EventLoopProxy,
    relay: ControlRelay,
) {
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        while let Ok(Some(frame)) = Frame::read(&mut input) {
            if frame.body.is_none()
                && let Some((command, value)) = crate::parse_host_control(&frame.line)
                && let Some(control) = super::events::host_control_from_parts(command, value)
            {
                if sender.send(OsrHostEvent::HostControl(control)).is_err() {
                    break;
                }
                proxy.wake_up();
                continue;
            }
            relay.forward(frame.to_bytes());
        }
    });
}
