use std::os::raw::c_ulong;

use x11_dl::xlib::{self, Xlib};

/// An active keyboard grab on the window while it has focus, so the window
/// manager's passive key grabs never see the keys.
pub(super) struct X11Inhibitor {
    xlib: Xlib,
    display: *mut xlib::Display,
    window: c_ulong,
    grabbed: bool,
}

impl X11Inhibitor {
    pub(super) fn new(
        display: *mut xlib::Display,
        window: c_ulong,
        focused: bool,
    ) -> Result<Self, String> {
        let xlib = Xlib::open().map_err(|error| format!("Could not load Xlib: {error}"))?;
        let mut inhibitor = Self {
            xlib,
            display,
            window,
            grabbed: false,
        };
        if focused && !inhibitor.grab() {
            return Err("Another app is holding the keyboard".to_string());
        }
        Ok(inhibitor)
    }

    pub(super) fn set_focused(&mut self, focused: bool) {
        if focused == self.grabbed {
            return;
        }
        if focused {
            self.grab();
        } else {
            self.ungrab();
        }
    }

    fn grab(&mut self) -> bool {
        let status = unsafe {
            (self.xlib.XGrabKeyboard)(
                self.display,
                self.window,
                xlib::True,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                xlib::CurrentTime,
            )
        };
        unsafe { (self.xlib.XFlush)(self.display) };
        self.grabbed = status == xlib::GrabSuccess;
        self.grabbed
    }

    fn ungrab(&mut self) {
        unsafe {
            (self.xlib.XUngrabKeyboard)(self.display, xlib::CurrentTime);
            (self.xlib.XFlush)(self.display);
        }
        self.grabbed = false;
    }
}

impl Drop for X11Inhibitor {
    fn drop(&mut self) {
        if self.grabbed {
            self.ungrab();
        }
    }
}
