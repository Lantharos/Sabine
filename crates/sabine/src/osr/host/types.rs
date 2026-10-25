use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use winit::event::MouseButton;

use crate::SabineWindowChrome;
use crate::osr::control::ControlWriter;
use crate::osr::protocol::OsrMessage;
use crate::osr::transport::IpcStream;

pub(super) const TITLEBAR_HEIGHT: f32 = 38.0;
pub(super) const CONTROL_SIZE: f32 = 24.0;
pub(super) const CONTROL_GAP: f32 = 8.0;
pub(super) const RESIZE_EDGE: f32 = 7.0;
pub(super) const CLOSE_GRACE: Duration = Duration::from_millis(300);
pub(super) const RESIZE_REPAINT_RETRY: Duration = Duration::from_millis(100);
pub(super) const RESIZE_REPAINT_GRACE: Duration = Duration::from_millis(900);
/// The longest a resized window waits for its page before showing the new
/// size with the last frame it has, so a busy page can't stall resizing.
pub(super) const RESIZE_HOLD_LIMIT: Duration = Duration::from_millis(100);
pub(super) const FALLBACK_ACTIVE_FRAME_RATE: u32 = 60;
/// Wayland briefly drops focus/occlusion around interactive move. Suspending
/// immediately thrashes lifecycle on the secondary (handed-off) window.
pub(super) const LIFECYCLE_SUSPEND_DEBOUNCE: Duration = Duration::from_millis(200);
/// How long a window that is already on screen and has a loading screen keeps
/// its last frame before showing it, such as after a reload.
pub(super) const RELOAD_SPLASH_DELAY: Duration = Duration::from_millis(120);
pub(super) const LOADING_FADE: Duration = Duration::from_millis(167);
/// The longest a page may hold back its first frame with `revealWhen`.
pub(super) const REVEAL_HOLD_LIMIT: Duration = Duration::from_secs(5);
pub(in crate::osr) const LOADING_ANIMATION_INTERVAL: Duration = Duration::from_millis(100);
pub(in crate::osr) const LOADING_MESSAGE_INTERVAL: Duration = Duration::from_millis(3_200);
pub(super) const TOOLTIP_REVEAL_DELAY: Duration = Duration::from_millis(500);

pub(super) const EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
pub(super) const EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
pub(super) const EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
pub(super) const EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;
pub(super) const EVENTFLAG_MIDDLE_MOUSE_BUTTON: u32 = 1 << 5;
pub(super) const EVENTFLAG_RIGHT_MOUSE_BUTTON: u32 = 1 << 6;
pub(super) const EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;
pub(super) const EVENTFLAG_IS_REPEAT: u32 = 1 << 13;
pub(super) const EVENTFLAG_PRECISION_SCROLLING_DELTA: u32 = 1 << 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WindowState {
    pub(super) shown: bool,
    pub(super) occluded: bool,
    pub(super) suspended: bool,
}

impl WindowState {
    pub(super) fn visible(self) -> bool {
        self.shown && !self.occluded
    }
}

/// Where a browser surface sits on the page, in DIPs, and its size in the
/// pixels Chromium painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SurfaceGeometry {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: u32,
    pub(super) height: u32,
}

impl SurfaceGeometry {
    pub(super) fn size(self) -> (u32, u32) {
        (self.width, self.height)
    }
}

pub(super) struct OverlayLayer {
    pub(super) geometry: SurfaceGeometry,
}

impl OverlayLayer {
    pub(super) fn new(geometry: SurfaceGeometry) -> Self {
        Self { geometry }
    }
}

pub(super) fn uses_sabine_chrome(chrome: SabineWindowChrome) -> bool {
    matches!(chrome, SabineWindowChrome::Sabine)
}

pub(super) enum OsrHostEvent {
    Connected(u64, IpcStream, Arc<ControlWriter>),
    Message(u64, OsrMessage),
    MessagesReady(u64, Arc<crate::osr::message_queue::MessageQueue>),
    HostControl(HostControl),
    Disconnected(u64),
    IncompatibleHost(u64),
    FileDialogClosed(u64, u32, Option<Vec<std::path::PathBuf>>),
    #[cfg(target_os = "linux")]
    RendererStarted(
        u64,
        Box<Result<crate::render::GpuRenderer, crate::render::RendererError>>,
    ),
    #[cfg(target_os = "linux")]
    DirectPresentation(u64, super::native::DirectOutcome),
    #[cfg(target_os = "linux")]
    SharedRuntimeOpened(u64, Result<(), String>),
}

impl OsrHostEvent {
    pub(super) fn connection_generation(&self) -> Option<u64> {
        match self {
            Self::Connected(generation, ..)
            | Self::Message(generation, _)
            | Self::MessagesReady(generation, _)
            | Self::Disconnected(generation)
            | Self::IncompatibleHost(generation)
            | Self::FileDialogClosed(generation, ..) => Some(*generation),
            #[cfg(target_os = "linux")]
            Self::SharedRuntimeOpened(generation, _) => Some(*generation),
            Self::HostControl(_) => None,
            #[cfg(target_os = "linux")]
            Self::RendererStarted(..) | Self::DirectPresentation(..) => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::osr) struct NativeLoading {
    pub(in crate::osr) started: Instant,
    pub(in crate::osr) splash_at: Option<Instant>,
    pub(in crate::osr) next_frame: Instant,
    pub(in crate::osr) message_seed: u64,
    pub(in crate::osr) ready_at: Option<Instant>,
    pub(in crate::osr) rejected_frame: Option<(u32, u32)>,
    pub(in crate::osr) slow_load_reported: bool,
    pub(in crate::osr) stall_reported: bool,
}

/// A loading screen fading out over the page that replaced it.
#[derive(Clone, Copy, Debug)]
pub(super) struct LoadingFade {
    pub(super) loading: NativeLoading,
    pub(super) started: Instant,
}

impl LoadingFade {
    /// The loading screen's opacity, easing out so the page shows at once.
    pub(super) fn opacity(self) -> Option<f32> {
        let progress = self.started.elapsed().as_secs_f32() / LOADING_FADE.as_secs_f32();
        (progress < 1.0).then(|| (1.0 - progress).powi(3))
    }
}

#[derive(Clone, Debug)]
pub(super) struct NativeTooltip {
    pub(super) text: Arc<str>,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) reveal_at: Instant,
    pub(super) shown: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ImeSurrounding {
    pub(super) text: String,
    pub(super) cursor: usize,
    pub(super) anchor: usize,
    pub(super) base_utf16: usize,
}

#[derive(Clone, Debug)]
pub(super) struct ImePreedit {
    pub(super) text: String,
    pub(super) selection: Option<(usize, usize)>,
}

impl NativeTooltip {
    pub(super) fn new(text: Arc<str>, x: f32, y: f32) -> Self {
        Self {
            text,
            x,
            y,
            reveal_at: Instant::now() + TOOLTIP_REVEAL_DELAY,
            shown: false,
        }
    }
}

impl NativeLoading {
    pub(in crate::osr) fn new(splash_delay: Option<Duration>) -> Self {
        let started = Instant::now();
        let splash_at = splash_delay.map(|delay| started + delay);
        Self {
            started,
            splash_at,
            next_frame: splash_at.unwrap_or(started),
            message_seed: next_loading_seed(),
            ready_at: None,
            rejected_frame: None,
            slow_load_reported: false,
            stall_reported: false,
        }
    }

    pub(super) fn splash_shown(self) -> bool {
        self.splash_at.is_some_and(|at| Instant::now() >= at)
    }
}

fn next_loading_seed() -> u64 {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    time ^ u64::from(std::process::id()) ^ SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TitlebarControl {
    Minimize,
    Maximize,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LifecycleState {
    Active,
    Suspended,
    Frozen,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum HostControl {
    Show,
    Hide,
    Focus(Option<String>),
    Visible(bool),
    Quit,
    ActivityBegin(HostActivity),
    ActivityEnd(HostActivity),
    Regions(sabine_platform::WindowRegions),
    SquareCorners(bool),
    Notify(sabine_platform::Notification),
    Print(super::print::PrintFiles),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HostActivity {
    pub(super) id: String,
    pub(super) keeps_running: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ControlRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

impl ControlRect {
    pub(super) fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MouseButtons {
    pub(super) left: bool,
    pub(super) middle: bool,
    pub(super) right: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ClickMemory {
    pub(super) button: MouseButton,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) at: Instant,
    pub(super) count: i32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingResizePaint {
    pub(super) size: (u32, u32),
    pub(super) retry_at: Instant,
    pub(super) deadline: Instant,
    /// Until when the window keeps its last frame on screen instead of
    /// showing the new size without a page painted at it.
    pub(super) held_until: Option<Instant>,
}
