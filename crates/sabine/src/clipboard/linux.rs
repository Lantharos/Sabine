use std::{path::PathBuf, sync::Arc};

use crossbeam_channel::Receiver;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::window::Window;

use super::wayland::WaylandClipboard;
use super::{ClipboardContent, Reply, Selection};

pub(crate) type Waker = Arc<dyn Fn() + Send + Sync>;

/// Files dragged over the window, on displays where the clipboard also
/// receives the window's drops.
pub(crate) enum DropEvent {
    Enter { paths: Vec<PathBuf>, x: f64, y: f64 },
    Motion { x: f64, y: f64 },
    Drop,
    Leave,
}

pub(crate) struct SystemClipboard {
    wayland: WaylandClipboard,
    drops: Receiver<DropEvent>,
}

impl SystemClipboard {
    pub(crate) fn connect(display: &dyn HasDisplayHandle, waker: Waker) -> Result<Self, String> {
        let (drop_sender, drops) = crossbeam_channel::unbounded();
        let RawDisplayHandle::Wayland(display) = display
            .display_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("this display has no clipboard".to_string());
        };
        let wayland =
            unsafe { WaylandClipboard::connect(display.display.as_ptr(), drop_sender, waker)? };
        Ok(Self { wayland, drops })
    }

    pub(crate) fn attach(&self, window: &dyn Window) {
        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::Wayland(surface) = handle.as_raw()
        {
            unsafe { self.wayland.attach(surface.surface.as_ptr()) };
        }
    }

    pub(crate) fn read(&self, selection: Selection, types: Option<Vec<String>>, reply: Reply) {
        self.wayland.read(selection, types, reply);
    }

    pub(crate) fn write(
        &self,
        selection: Selection,
        content: ClipboardContent,
    ) -> Result<(), String> {
        self.wayland.write(selection, content);
        Ok(())
    }

    /// Whether the window's drops arrive here instead of through the window
    /// toolkit.
    pub(crate) fn owns_drops(&self) -> bool {
        self.wayland.owns_drops()
    }

    pub(crate) fn set_outgoing_drag(&self, active: bool) {
        self.wayland.set_outgoing_drag(active);
    }

    pub(crate) fn drop_events(&self) -> impl Iterator<Item = DropEvent> + '_ {
        self.drops.try_iter()
    }
}
