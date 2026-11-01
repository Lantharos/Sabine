use std::sync::mpsc;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::UI::ViewManagement::InputPane;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::WinRT::IInputPaneInterop;

use crate::osr::host::native::OsrNativeHost;

/// The Windows touch keyboard, which comes up when a tap puts the page's
/// focus in a field, as it does for the desktop's own fields.
#[derive(Default)]
pub(in crate::osr::host) struct TouchKeyboard {
    pane: Option<mpsc::Sender<bool>>,
    tapped: bool,
    shown: bool,
}

impl TouchKeyboard {
    /// Notes whether the latest press came from a finger or pen rather than
    /// a mouse.
    pub(in crate::osr::host) fn note_press(&mut self, tapped: bool) {
        self.tapped = tapped;
    }

    pub(in crate::osr::host) fn forget_window(&mut self) {
        self.pane = None;
        self.shown = false;
    }
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn sync_touch_keyboard(&mut self, editing: bool) {
        let keyboard = &self.touch_keyboard;
        let show = match (editing, keyboard.shown) {
            (true, false) if keyboard.tapped => true,
            (false, true) => false,
            _ => return,
        };
        if self.touch_keyboard.pane.is_none() {
            let Some(window) = self.window.as_ref() else {
                return;
            };
            let Ok(RawWindowHandle::Win32(handle)) =
                window.window_handle().map(|handle| handle.as_raw())
            else {
                return;
            };
            self.touch_keyboard.pane = Some(start_input_pane(handle.hwnd.get()));
        }
        let sent = self
            .touch_keyboard
            .pane
            .as_ref()
            .is_some_and(|pane| pane.send(show).is_ok());
        self.touch_keyboard.shown = show && sent;
    }
}

/// Asks the window's input pane to show or hide the keyboard from a thread
/// of its own, so the window never waits on the shell.
fn start_input_pane(window: isize) -> mpsc::Sender<bool> {
    let (requests, received) = mpsc::channel::<bool>();
    let started = std::thread::Builder::new()
        .name("sabine-touch-keyboard".to_string())
        .spawn(move || {
            let pane = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .and_then(|()| windows_core::factory::<InputPane, IInputPaneInterop>())
                .and_then(|interop| unsafe {
                    interop.GetForWindow::<InputPane>(HWND(window as _))
                });
            let pane = match pane {
                Ok(pane) => pane,
                Err(error) => {
                    sabine_runtime::report_error(
                        "input",
                        format!("the touch keyboard is unavailable: {error}"),
                    );
                    return;
                }
            };
            for show in received {
                let shown = if show { pane.TryShow() } else { pane.TryHide() };
                if let Err(error) = shown {
                    sabine_runtime::report_error(
                        "input",
                        format!("could not move the touch keyboard: {error}"),
                    );
                }
            }
        });
    if let Err(error) = started {
        sabine_runtime::report_error(
            "input",
            format!("the touch keyboard is unavailable: {error}"),
        );
    }
    requests
}
