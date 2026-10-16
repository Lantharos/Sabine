mod geometry;
mod launch;
mod window;

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    process::Child,
    sync::{Arc, mpsc},
    time::Instant,
};

use sabine_platform::{WindowChrome as PlatformWindowChrome, WindowOptions, WindowRegionRect};
use winit::{
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::Window as WinitWindow,
};

use crate::bridge::frame::Frame;
use crate::osr::control::{ControlRelay, ControlWriter};

use crate::SabineWindowChrome;
use crate::osr::transport::IpcStream;
use crate::render::{BgraImage, DisplayList, GpuRenderer, ImageId};
use sabine_platform::WindowEffect;

use super::config::OsrHostConfig;
use super::socket::SocketReader;
use super::types::{
    ClickMemory, LifecycleState, MouseButtons, OsrHostEvent, OverlayLayer, PendingResizePaint,
    SurfaceGeometry, TitlebarControl,
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
    pub(super) retained_frames: HashMap<ImageId, BgraImage>,
    pub(super) overlays: BTreeMap<Arc<str>, OverlayLayer>,
    pub(super) display_list: DisplayList,
    pub(super) page_drag_regions: Vec<WindowRegionRect>,
    pub(super) page_drag_exclusion_regions: Vec<WindowRegionRect>,
    pub(super) hovered_control: Option<TitlebarControl>,
    pub(super) pressed_control: Option<TitlebarControl>,
    pub(super) cursor: super::ui::cursor::CursorState,
    pub(super) modifiers: winit::keyboard::ModifiersState,
    pub(super) mouse: MouseButtons,
    pub(super) touch: super::input::TouchState,
    pub(super) wheel_remainder: super::input::WheelRemainder,
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
    pub(super) freeze_deadline: Option<Instant>,
    pub(super) closing_deadline: Option<Instant>,
    pub(super) pending_resize_paint: Option<PendingResizePaint>,
    pub(super) pending_suspend_at: Option<Instant>,
    pub(super) effect_regions_dirty: bool,
    pub(super) running_activities: BTreeSet<String>,
    pub(super) page_media_playing: bool,
    pub(super) presented: bool,
    pub(super) main_frame_presented: bool,
    pub(super) loading: Option<super::types::NativeLoading>,
    pub(super) tooltip: Option<super::types::NativeTooltip>,
    pub(super) context_menu: Option<super::ui::context_menu::ContextMenu>,
    #[cfg(target_os = "linux")]
    pub(super) pending_activation_token: Option<winit::window::ActivationToken>,
    pub(super) drag: super::input::DragState,
    pub(super) shortcuts: super::input::ShortcutInhibition,
    /// CEF exited with process-singleton handoff (code 24). The existing
    /// browser process owns this window's OSR endpoint; keep listening.
    pub(super) cef_handed_off: bool,
    /// Deadline for the primary CEF process to connect after exit-24 handoff.
    pub(super) handoff_deadline: Option<Instant>,
    #[cfg(target_os = "macos")]
    pub(super) surface_broker: Option<crate::osr::accel::SurfaceBroker>,
    pub(super) media: crate::media::MediaHost,
    pub(super) clipboard: Option<crate::clipboard::SystemClipboard>,
    #[cfg(target_os = "linux")]
    pub(super) placement: Option<sabine_platform::WindowPlacement>,
    #[cfg(target_os = "linux")]
    pub(super) appearance: sabine_platform::AppearanceWatcher,
    pub(super) published_appearance: Option<sabine_platform::Appearance>,
    pub(super) notifier: Option<sabine_platform::Notifier>,
    #[cfg(target_os = "linux")]
    pub(super) paste_gesture: Option<Instant>,
    #[cfg(target_os = "linux")]
    pub(super) software_paint: bool,
    #[cfg(target_os = "linux")]
    pub(super) accelerated_launch: bool,
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
        #[cfg(target_os = "linux")]
        let software_paint = !sabine_runtime::runtime_version_at_least(
            &config.runtime_dir,
            &sabine_runtime::MIN_LINUX_SHARED_TEXTURE_CEF,
        );
        let focused = visible && config.active;
        let lifecycle_state = if visible {
            LifecycleState::Active
        } else {
            LifecycleState::Suspended
        };
        #[cfg(target_os = "linux")]
        let appearance_proxy = proxy.clone();
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
            retained_frames: HashMap::new(),
            overlays: BTreeMap::new(),
            display_list: DisplayList::default(),
            page_drag_regions: Vec::new(),
            page_drag_exclusion_regions: Vec::new(),
            hovered_control: None,
            pressed_control: None,
            cursor: Default::default(),
            modifiers: Default::default(),
            mouse: MouseButtons::default(),
            touch: Default::default(),
            wheel_remainder: Default::default(),
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
            freeze_deadline: None,
            closing_deadline: None,
            pending_resize_paint: None,
            pending_suspend_at: None,
            effect_regions_dirty: false,
            running_activities: BTreeSet::new(),
            page_media_playing: false,
            presented: false,
            main_frame_presented: false,
            loading: visible.then(super::types::NativeLoading::new),
            tooltip: None,
            context_menu: None,
            #[cfg(target_os = "linux")]
            pending_activation_token: None,
            drag: Default::default(),
            shortcuts: Default::default(),
            cef_handed_off: false,
            handoff_deadline: None,
            #[cfg(target_os = "macos")]
            surface_broker: None,
            media: Default::default(),
            clipboard: None,
            #[cfg(target_os = "linux")]
            placement: None,
            #[cfg(target_os = "linux")]
            appearance: sabine_platform::AppearanceWatcher::start(move || {
                appearance_proxy.wake_up()
            }),
            published_appearance: None,
            notifier: None,
            #[cfg(target_os = "linux")]
            paste_gesture: None,
            #[cfg(target_os = "linux")]
            software_paint,
            #[cfg(target_os = "linux")]
            accelerated_launch: false,
        }
    }

    pub(super) fn window_options(&self) -> WindowOptions {
        WindowOptions {
            title: self.config.title.to_string(),
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

    pub(super) fn send_control(&self, line: impl Into<String>) {
        let Some(writer) = &self.control_writer else {
            return;
        };
        if let Err(error) = writer.send(line.into()) {
            sabine_runtime::report_error("window", format!("browser control send failed: {error}"));
        }
    }

    pub(super) fn send_mouse_motion(&self, line: String) {
        let Some(writer) = &self.control_writer else {
            return;
        };
        if let Err(error) = writer.send_motion(line) {
            sabine_runtime::report_error("window", format!("pointer send failed: {error}"));
        }
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
