use serde::{Deserialize, Serialize};

/// A window size in logical pixels, the CSS pixels of the page at its
/// default zoom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

impl WindowSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// Changes to an open window, which the app's Rust code sends with
/// `BridgeEventEmitter::control_window` and its pages with
/// `sabine.window.control`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WindowControl {
    /// Names the window from now on, whatever its page's `document.title`
    /// says. `None` lets the page's title name the window again.
    SetTitle {
        title: Option<String>,
    },
    SetSize {
        width: u32,
        height: u32,
    },
    /// `None` lets the window shrink to any size.
    SetMinSize {
        size: Option<WindowSize>,
    },
    /// `None` lets the window grow to any size.
    SetMaxSize {
        size: Option<WindowSize>,
    },
    /// Moves the window's outer top left corner to a point of the desktop, in
    /// the physical pixels display bounds are in. Wayland places windows
    /// itself, so there it fails.
    SetPosition {
        x: i32,
        y: i32,
    },
    /// Keeps the window above other windows. On Wayland this needs a
    /// compositor with `kestrel_window_v1`.
    SetAlwaysOnTop {
        enabled: bool,
    },
    SetResizable {
        enabled: bool,
    },
    Minimize,
    Maximize,
    Unmaximize,
    SetFullscreen {
        enabled: bool,
    },
    /// Asks people to look at the window: the Dock icon bounces once on
    /// macOS, the taskbar button flashes on Windows until the window is
    /// focused, and Wayland compositors mark the window. A critical request
    /// keeps the Dock icon bouncing until the app is active, and also
    /// flashes the window's frame on Windows.
    RequestAttention {
        critical: bool,
    },
}
