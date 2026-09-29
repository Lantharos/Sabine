//! The desktop clipboard and primary selection on Linux. Chromium renders
//! offscreen and holds no focused window of its own, so the window host owns
//! the selections and pages reach them through it.

mod content;
mod pipe;
mod wayland;
mod x11;

use std::{path::PathBuf, sync::Arc};

use crossbeam_channel::Receiver;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::{event_loop::DndAction, window::Window};

pub(crate) use content::{ClipboardContent, Selection};
use wayland::WaylandClipboard;
use x11::X11Clipboard;

pub(crate) type Reply = Box<dyn FnOnce(Result<ClipboardContent, String>) + Send>;
pub(crate) type Waker = Arc<dyn Fn() + Send + Sync>;

/// Files dragged over the window, on displays where the clipboard also
/// receives the window's drops.
pub(crate) enum DropEvent {
    Enter {
        paths: Vec<PathBuf>,
        x: f64,
        y: f64,
    },
    Motion {
        x: f64,
        y: f64,
        action: Option<DndAction>,
    },
    Drop {
        action: Option<DndAction>,
    },
    Leave,
}

enum Backend {
    Wayland(WaylandClipboard),
    X11(X11Clipboard),
}

pub(crate) struct SystemClipboard {
    backend: Backend,
    drops: Receiver<DropEvent>,
}

impl SystemClipboard {
    pub(crate) fn connect(display: &dyn HasDisplayHandle, waker: Waker) -> Result<Self, String> {
        let (drop_sender, drops) = crossbeam_channel::unbounded();
        let display = display
            .display_handle()
            .map_err(|error| error.to_string())?
            .as_raw();
        let backend = match display {
            RawDisplayHandle::Wayland(display) => Backend::Wayland(unsafe {
                WaylandClipboard::connect(display.display.as_ptr(), drop_sender, waker)?
            }),
            RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => {
                Backend::X11(X11Clipboard::connect()?)
            }
            _ => return Err("this display has no clipboard".to_string()),
        };
        Ok(Self { backend, drops })
    }

    pub(crate) fn attach(&self, window: &dyn Window) {
        if let Backend::Wayland(clipboard) = &self.backend
            && let Ok(handle) = window.window_handle()
            && let RawWindowHandle::Wayland(surface) = handle.as_raw()
        {
            unsafe { clipboard.attach(surface.surface.as_ptr()) };
        }
    }

    pub(crate) fn read(&self, selection: Selection, types: Option<Vec<String>>, reply: Reply) {
        match &self.backend {
            Backend::Wayland(clipboard) => clipboard.read(selection, types, reply),
            Backend::X11(clipboard) => clipboard.read(selection, types, reply),
        }
    }

    pub(crate) fn write(&self, selection: Selection, content: ClipboardContent) {
        match &self.backend {
            Backend::Wayland(clipboard) => clipboard.write(selection, content),
            Backend::X11(clipboard) => clipboard.write(selection, content),
        }
    }

    /// Whether the window's drops arrive here instead of through the window
    /// toolkit.
    pub(crate) fn owns_drops(&self) -> bool {
        matches!(&self.backend, Backend::Wayland(clipboard) if clipboard.owns_drops())
    }

    pub(crate) fn set_outgoing_drag(&self, active: bool) {
        if let Backend::Wayland(clipboard) = &self.backend {
            clipboard.set_outgoing_drag(active);
        }
    }

    pub(crate) fn drop_events(&self) -> impl Iterator<Item = DropEvent> + '_ {
        self.drops.try_iter()
    }
}
