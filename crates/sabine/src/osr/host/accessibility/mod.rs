//! The page as assistive technology reads it. Chromium renders the window
//! offscreen and so exposes no accessibility of its own; the window instead
//! asks the browser for the page's accessibility tree once assistive
//! technology reads it, keeps a copy of every frame's tree, and hands the
//! desktop the changes as they come.

mod actions;
mod adapter;
mod bounds;
mod frame;
mod node;
mod page;
mod roles;
#[cfg(test)]
mod tests;
mod text;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};

use accesskit::{NodeId, Vec2};

use adapter::{Adapter, Request, Requests};
use page::{PageTree, WindowGeometry};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::accessibility::{AxLocationChange, AxTreeChange};

/// The window itself, above the page's frames.
const WINDOW: NodeId = NodeId(0);

/// A page node's id: its frame in the high half, Chromium's id in the low.
fn node_id(frame: u32, id: i32) -> NodeId {
    NodeId((u64::from(frame) << 32) | u64::from(id as u32))
}

fn split_node_id(id: NodeId) -> (u32, i32) {
    ((id.0 >> 32) as u32, id.0 as u32 as i32)
}

pub(in crate::osr::host) struct Accessibility {
    page: Arc<Mutex<PageTree>>,
    adapter: Option<Adapter>,
    sender: mpsc::Sender<Request>,
    requests: mpsc::Receiver<Request>,
    /// Whether the browser builds the page's tree, which it does only while
    /// assistive technology reads the window.
    building: bool,
}

impl Default for Accessibility {
    fn default() -> Self {
        let (sender, requests) = mpsc::channel();
        Self {
            page: Arc::default(),
            adapter: None,
            sender,
            requests,
            building: false,
        }
    }
}

impl Accessibility {
    fn page(&self) -> MutexGuard<'_, PageTree> {
        self.page.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Hands the desktop what changed, outside the lock the desktop's thread
    /// takes to read the whole tree.
    fn publish(&mut self) {
        let update = self.page().take_update();
        if let Some(adapter) = &mut self.adapter {
            adapter.update(update);
        }
    }
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn attach_accessibility(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        self.sync_accessibility_window();
        let requests = Requests {
            page: Arc::clone(&self.accessibility.page),
            sender: self.accessibility.sender.clone(),
            proxy: self.proxy.clone(),
        };
        self.accessibility.adapter = Adapter::new(window.as_ref(), requests);
        #[cfg(target_os = "linux")]
        self.accessibility_window_moved();
        #[cfg(not(windows))]
        self.accessibility_focus_changed(self.focused);
    }

    pub(in crate::osr::host) fn detach_accessibility(&mut self) {
        self.accessibility.adapter = None;
    }

    pub(in crate::osr::host) fn handle_accessibility_requests(&mut self) {
        while let Ok(request) = self.accessibility.requests.try_recv() {
            match request {
                Request::Activated => {
                    if !self.accessibility.building {
                        self.accessibility.building = true;
                        self.send_control("accessibility\t1\n");
                    }
                }
                Request::Deactivated => {
                    self.accessibility.building = false;
                    self.send_control("accessibility\t0\n");
                    self.accessibility.page().clear();
                }
                Request::Action(request) => {
                    let control = actions::control(&self.accessibility.page(), &request);
                    if let Some(control) = control {
                        self.send_control(control);
                    }
                }
            }
        }
    }

    pub(in crate::osr::host) fn apply_accessibility_tree(&mut self, change: AxTreeChange) {
        if self.accessibility.building {
            self.accessibility.page().apply_tree(change);
            self.accessibility.publish();
        }
    }

    pub(in crate::osr::host) fn move_accessibility_nodes(&mut self, change: AxLocationChange) {
        if self.accessibility.building {
            self.accessibility.page().move_nodes(change);
            self.accessibility.publish();
        }
    }

    /// A new browser builds the page's tree only once asked to.
    pub(in crate::osr::host) fn accessibility_browser_connected(&self) {
        if self.accessibility.building {
            self.send_control("accessibility\t1\n");
        }
    }

    pub(in crate::osr::host) fn accessibility_browser_lost(&mut self) {
        self.accessibility.page().clear();
        self.accessibility.publish();
    }

    pub(in crate::osr::host) fn sync_accessibility_window(&mut self) {
        let scale = self.scale();
        let geometry = WindowGeometry {
            title: self.config.title.to_string(),
            width: f64::from(self.surface_size.width),
            height: f64::from(self.surface_size.height),
            page_origin: Vec2::new(0.0, f64::from(self.titlebar_height()) * scale),
        };
        self.accessibility.page().set_window(geometry);
        self.accessibility.publish();
        #[cfg(target_os = "linux")]
        self.accessibility_window_moved();
    }

    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn accessibility_window_moved(&mut self) {
        if let (Some(adapter), Some(window)) = (&mut self.accessibility.adapter, &self.window) {
            adapter.moved(window.as_ref());
        }
    }

    #[cfg(not(windows))]
    pub(in crate::osr::host) fn accessibility_focus_changed(&mut self, focused: bool) {
        if let Some(adapter) = &mut self.accessibility.adapter {
            adapter.focus_changed(focused);
        }
    }
}
