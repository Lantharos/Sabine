use std::time::Instant;

use winit::event_loop::{ActiveEventLoop, ControlFlow};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{LifecycleState, RESIZE_HOLD_LIMIT};

impl OsrNativeHost {
    pub(in crate::osr::host) fn drive_resize_paint(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
    ) -> bool {
        let Some(mut pending) = self.pending_resize_paint else {
            return false;
        };
        if !self.config.visible
            || self.lifecycle_state != LifecycleState::Active
            || self.window.is_none()
        {
            self.pending_resize_paint = None;
            return false;
        }
        if self.main_surface_matches(pending.size) {
            self.pending_resize_paint = None;
            return false;
        }
        let now = Instant::now();
        if now >= pending.deadline {
            self.pending_resize_paint = None;
            if self.presented {
                self.render();
            }
            return false;
        }
        if pending.held_until.is_some_and(|held| now >= held) {
            pending.held_until = None;
            self.pending_resize_paint = Some(pending);
            self.render();
        }
        if now >= pending.retry_at {
            self.retry_resize_paint();
        }
        if let Some(pending) = self.pending_resize_paint {
            let wake = pending.retry_at.min(pending.deadline);
            let wake = pending.held_until.map_or(wake, |held| held.min(wake));
            event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
            return true;
        }
        false
    }

    /// On Wayland a window shows its new size from the commit after the
    /// configure, so the window commits a new size only together with a page
    /// painted at it. A resize that arrives while the page is still catching
    /// up with an earlier one, as while dragging an edge, shows at once with
    /// the last frame the page painted, at the pace the compositor asks.
    pub(in crate::osr::host) fn resize_hold(&self, now: Instant) -> Option<Instant> {
        (cfg!(target_os = "linux") && self.presented && self.pending_resize_paint.is_none())
            .then_some(now + RESIZE_HOLD_LIMIT)
    }

    /// Whether the window is waiting for a page frame at its new size
    /// before it shows that size.
    pub(in crate::osr::host) fn resize_held(&self) -> bool {
        self.pending_resize_paint
            .and_then(|pending| pending.held_until)
            .is_some_and(|held| Instant::now() < held)
    }
}
