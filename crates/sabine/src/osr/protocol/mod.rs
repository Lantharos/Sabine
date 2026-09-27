mod config_json;
mod encode;
mod wire;

pub(crate) use config_json::{
    control_regions_from_json, control_regions_to_json, lifecycle_from_json, lifecycle_to_json,
    rects_from_json, rects_to_json, regions_from_json, regions_to_json,
};
pub(crate) use encode::encode_component;
#[cfg(unix)]
pub(crate) use wire::PaintSlots;
pub(crate) use wire::WireReader;

use sabine_platform::WindowRegionRect;
use std::{ops::Range, sync::Arc};

#[cfg(unix)]
use self::wire::PaintLease;

pub(crate) const MAIN_TEXTURE_ID: &str = "__sabine_main";
pub(crate) const POPUP_TEXTURE_ID: &str = "__sabine_popup";
pub(crate) const POPUP_OVERLAY_ID: &str = "__sabine_popup";

#[derive(Clone, Debug)]
pub(crate) enum FrameBytes {
    Inline {
        source: Arc<Vec<u8>>,
        range: Range<usize>,
    },
    #[cfg(unix)]
    Shared {
        source: Arc<PaintLease>,
        range: Range<usize>,
    },
}

impl FrameBytes {
    pub(crate) fn as_slice(&self) -> &[u8] {
        match self {
            Self::Inline { source, range } => &source[range.clone()],
            #[cfg(unix)]
            Self::Shared { source, range } => &source.as_slice()[range.clone()],
        }
    }
}

#[derive(Debug)]
pub(crate) enum OsrMessage {
    PaintBatch(OsrPaintBatch),
    #[cfg(any(windows, target_os = "macos"))]
    AccelFrame(OsrAccelFrame),
    /// Hide the built-in popup overlay (`__sabine_popup`).
    PopupHidden,
    /// Hide a guest overlay by id.
    GuestHidden(String),
    GuestCaptureRequested {
        browser_id: String,
        request_id: String,
        guest_id: String,
    },
    DraggableRegionsChanged {
        drag: Vec<WindowRegionRect>,
        exclusion: Vec<WindowRegionRect>,
    },
    Cursor(String),
    CloseRequested,
    StartDragRequested,
    MinimizeRequested,
    ToggleMaximizeRequested,
    MaximizeRequested,
    RestoreRequested,
    FullscreenRequested(bool),
    ShowRequested,
    HideRequested,
    FocusRequested(Option<String>),
    FileDragRequested(FileDragRequest),
    MainLoadStarted,
    MainLoadReady,
    FatalError(String),
    ImeStateChanged(u32),
    ImeCursorAreaChanged {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    TooltipChanged(String),
    ImeSurroundingChanged {
        text: String,
        cursor_utf16: usize,
        anchor_utf16: usize,
        base_utf16: usize,
    },
    /// Full `SABINE_BRIDGE_REQUEST\t...` line from the owning CEF handler.
    BridgeRequest(String),
}

#[derive(Clone, Debug)]
pub(crate) struct FileDragRequest {
    pub paths: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct OsrPaintBatch {
    pub surface: OsrSurface,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub rects: Vec<PaintRect>,
}

#[cfg(any(windows, target_os = "macos"))]
#[derive(Debug)]
pub(crate) struct OsrAccelFrame {
    pub surface: OsrSurface,
    pub coded_width: u32,
    pub coded_height: u32,
    pub visible_x: u32,
    pub visible_y: u32,
    pub visible_width: u32,
    pub visible_height: u32,
    pub x: i32,
    pub y: i32,
    pub format: u32,
    /// Windows: duplicated NT handle of a Sabine-owned texture. macOS: shared surface id.
    pub native_handle: u64,
    /// Producer slot released only after the compositor finishes sampling it.
    pub slot_token: u64,
    #[cfg(target_os = "macos")]
    pub io_surface: Option<crate::osr::accel::SharedSurface>,
}

#[cfg(windows)]
impl Drop for OsrAccelFrame {
    fn drop(&mut self) {
        crate::osr::accel::close_imported_handle(self.native_handle);
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PaintRect {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub bytes: FrameBytes,
}

impl PaintRect {
    pub(crate) fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OsrSurface {
    Main,
    /// Built-in popup overlay (also addressable as guest id `__sabine_popup`).
    Popup,
    /// Named guest overlay composited above the main surface.
    Guest(String),
}

impl OsrSurface {
    pub(crate) fn overlay_id(&self) -> Option<&str> {
        match self {
            Self::Main => None,
            Self::Popup => Some(POPUP_OVERLAY_ID),
            Self::Guest(id) => Some(id.as_str()),
        }
    }
}
