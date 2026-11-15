//! The desktop's accessibility for one window: AT-SPI on Linux, UI
//! Automation on Windows and NSAccessibility on macOS. Each activates once
//! assistive technology reads the window.

use std::sync::{Arc, Mutex, PoisonError, mpsc};

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
#[cfg(not(target_os = "linux"))]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::{event_loop::EventLoopProxy, window::Window};

use super::page::PageTree;

/// What the desktop's accessibility asks of the window, off its own thread.
pub(super) enum Request {
    Activated,
    Deactivated,
    Action(ActionRequest),
}

#[derive(Clone)]
pub(super) struct Requests {
    pub page: Arc<Mutex<PageTree>>,
    pub sender: mpsc::Sender<Request>,
    pub proxy: EventLoopProxy,
}

impl Requests {
    fn send(&self, request: Request) {
        if self.sender.send(request).is_ok() {
            self.proxy.wake_up();
        }
    }
}

impl ActivationHandler for Requests {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let tree = self
            .page
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .full_update();
        self.send(Request::Activated);
        Some(tree)
    }
}

impl ActionHandler for Requests {
    fn do_action(&mut self, request: ActionRequest) {
        self.send(Request::Action(request));
    }
}

impl DeactivationHandler for Requests {
    fn deactivate_accessibility(&mut self) {
        self.send(Request::Deactivated);
    }
}

pub(super) struct Adapter {
    #[cfg(target_os = "linux")]
    inner: accesskit_unix::Adapter,
    #[cfg(windows)]
    inner: accesskit_windows::SubclassingAdapter,
    #[cfg(target_os = "macos")]
    inner: accesskit_macos::SubclassingAdapter,
}

impl Adapter {
    #[cfg(target_os = "linux")]
    pub fn new(_window: &dyn Window, requests: Requests) -> Option<Self> {
        Some(Self {
            inner: accesskit_unix::Adapter::new(requests.clone(), requests.clone(), requests),
        })
    }

    #[cfg(windows)]
    pub fn new(window: &dyn Window, requests: Requests) -> Option<Self> {
        let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        let hwnd = accesskit_windows::HWND(handle.hwnd.get() as *mut _);
        Some(Self {
            inner: accesskit_windows::SubclassingAdapter::new(hwnd, requests.clone(), requests),
        })
    }

    #[cfg(target_os = "macos")]
    pub fn new(window: &dyn Window, requests: Requests) -> Option<Self> {
        let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: the pointer is the window's live content view, which the
        // adapter retains.
        let inner = unsafe {
            accesskit_macos::SubclassingAdapter::new(
                handle.ns_view.as_ptr(),
                requests.clone(),
                requests,
            )
        };
        Some(Self { inner })
    }

    pub fn update(&mut self, update: TreeUpdate) {
        #[cfg(target_os = "linux")]
        self.inner.update_if_active(|| update);
        #[cfg(not(target_os = "linux"))]
        if let Some(events) = self.inner.update_if_active(|| update) {
            events.raise();
        }
    }

    /// Windows learns of focus from the window's own messages.
    #[cfg(not(windows))]
    pub fn focus_changed(&mut self, focused: bool) {
        #[cfg(target_os = "linux")]
        self.inner.update_window_focus_state(focused);
        #[cfg(target_os = "macos")]
        if let Some(events) = self.inner.update_view_focus_state(focused) {
            events.raise();
        }
    }

    /// Where the window and its contents are on the screen, which Linux
    /// adapters can't learn from the window itself.
    #[cfg(target_os = "linux")]
    pub fn moved(&mut self, window: &dyn Window) {
        let size = window.surface_size();
        let position = window
            .outer_position()
            .map(|position| (f64::from(position.x), f64::from(position.y)))
            .unwrap_or_default();
        let bounds = accesskit::Rect::from_origin_size(
            position,
            (f64::from(size.width), f64::from(size.height)),
        );
        self.inner.set_root_window_bounds(bounds, bounds);
    }
}
