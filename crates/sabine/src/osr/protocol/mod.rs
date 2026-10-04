mod config_json;
mod encode;
mod wire;

pub(crate) use config_json::{
    control_regions_from_json, control_regions_to_json, lifecycle_from_json, lifecycle_to_json,
    notification_from_json, notification_to_json, rects_from_json, rects_to_json,
    regions_from_json, regions_to_json,
};
pub(crate) use encode::encode_component;
#[cfg(unix)]
pub(crate) use wire::PaintSlots;
pub(crate) use wire::WireReader;

use sabine_platform::WindowRegionRect;
use std::{ops::Range, sync::Arc};

#[cfg(unix)]
use self::wire::PaintLease;

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
    Cursor(PageCursorMessage),
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
    DragStarted {
        content: DragContent,
        operations: u32,
    },
    /// The drop the page under the pointer would accept.
    DragOperation(u32),
    ContextMenu {
        x: i32,
        y: i32,
        items: Vec<ContextMenuItem>,
    },
    ContextMenuDismissed,
    FileDialog(FileDialogRequest),
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
    BridgeRequest(crate::bridge::frame::Frame),
}

/// A cursor the page asked for: one of CEF's cursor types, or an image.
#[derive(Debug)]
pub(crate) enum PageCursorMessage {
    Named(u32),
    Custom(CursorImage),
}

#[derive(Debug, Hash)]
pub(crate) struct CursorImage {
    pub rgba: Vec<u8>,
    pub width: u16,
    pub height: u16,
    pub hotspot_x: u16,
    pub hotspot_y: u16,
}

/// One entry of a page's context menu, as Chromium offers it.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub(crate) struct ContextMenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
}

/// A page's request to choose files, as Chromium describes its file input.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileDialogRequest {
    pub id: u32,
    pub mode: FileDialogMode,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub default_path: Option<std::path::PathBuf>,
    #[serde(default)]
    pub filters: Vec<FileDialogFilter>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum FileDialogMode {
    Open,
    OpenMultiple,
    OpenFolder,
    Save,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct FileDialogFilter {
    pub description: String,
    pub extensions: Vec<String>,
}

/// Content dragged out of a page or dropped into one.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub(crate) struct DragContent {
    pub text: String,
    pub html: String,
    pub url: String,
    pub files: Vec<std::path::PathBuf>,
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
    /// Stable identity of the Sabine-owned texture this frame was copied into.
    pub resource_id: u64,
    /// Producer slot holding that texture; imports are kept per slot.
    pub resource_slot: u32,
    /// Producer slot released only after the compositor finishes sampling it.
    pub slot_token: u64,
    pub resource: Option<crate::osr::accel::SharedResource>,
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
