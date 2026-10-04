//! Linux's window host owns the clipboard and primary selection, so it also
//! decides when pages outside the app may read them: while the user pastes.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use winit::{
    event::KeyEvent,
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, NamedKey, PhysicalKey},
    window::Window,
};

use crate::clipboard::SystemClipboard;
use crate::osr::host::native::OsrNativeHost;

/// How long after the user asks to paste a page outside the app may read
/// the clipboard.
const PASTE_GESTURE: Duration = Duration::from_secs(2);

impl OsrNativeHost {
    /// Connects before the browser starts, so its pages know they can reach
    /// the clipboard.
    pub(in crate::osr::host) fn connect_clipboard(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.clipboard.is_some() {
            return;
        }
        let proxy = self.proxy.clone();
        match SystemClipboard::connect(
            event_loop.rwh_06_handle(),
            Arc::new(move || proxy.wake_up()),
        ) {
            Ok(clipboard) => {
                self.clipboard = Some(clipboard);
                self.config.bridge_policy["clipboard"] = true.into();
            }
            Err(error) => sabine_runtime::report_error("clipboard", error),
        }
    }

    pub(in crate::osr::host) fn attach_clipboard(&self, window: &dyn Window) {
        if let Some(clipboard) = &self.clipboard {
            clipboard.attach(window);
        }
    }

    pub(in crate::osr::host) fn note_paste_key(&mut self, event: &KeyEvent) {
        let control = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        let paste = (control && event.physical_key == PhysicalKey::Code(KeyCode::KeyV))
            || (shift && event.logical_key == NamedKey::Insert);
        if paste {
            self.paste_gesture = Some(Instant::now());
        }
    }

    pub(in crate::osr::host) fn note_middle_click(&mut self) {
        self.paste_gesture = Some(Instant::now());
    }

    pub(super) fn pasting(&self) -> bool {
        self.paste_gesture
            .is_some_and(|at| at.elapsed() < PASTE_GESTURE)
    }
}
