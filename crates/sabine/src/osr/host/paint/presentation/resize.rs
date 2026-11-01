use std::time::Instant;

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{
    LifecycleState, PendingResizePaint, RESIZE_HOLD_LIMIT, RESIZE_REPAINT_GRACE,
    RESIZE_REPAINT_RETRY, SurfaceGeometry,
};

impl OsrNativeHost {
    pub(in crate::osr::host) fn send_resize(&self) {
        let viewport = self.page_viewport();
        self.send_control(format!(
            "resize\t{}\t{}\t{}\n",
            viewport.width,
            viewport.height,
            viewport.scale_argument()
        ));
    }

    pub(in crate::osr::host) fn queue_resize_paint(&mut self) {
        let size = self.page_viewport().pixel_size();
        if self.main_surface_matches(size) {
            self.pending_resize_paint = None;
            return;
        }
        let now = Instant::now();
        self.pending_resize_paint = Some(PendingResizePaint {
            size,
            retry_at: now + RESIZE_REPAINT_RETRY,
            deadline: now + RESIZE_REPAINT_GRACE,
            held_until: self.resize_hold(now),
        });
        self.send_resize();
    }

    pub(in crate::osr::host) fn retry_resize_paint(&mut self) {
        let Some(mut pending) = self.pending_resize_paint else {
            return;
        };
        if self.main_surface_matches(pending.size) {
            self.pending_resize_paint = None;
            return;
        }
        let now = Instant::now();
        if now < pending.retry_at {
            return;
        }
        self.send_resize();
        pending.retry_at = now + RESIZE_REPAINT_RETRY;
        self.pending_resize_paint = Some(pending);
    }

    pub(in crate::osr::host) fn clear_pending_resize_paint(&mut self) {
        if self
            .pending_resize_paint
            .is_some_and(|pending| self.main_surface_matches(pending.size))
        {
            self.pending_resize_paint = None;
        }
    }

    /// Whether a main frame was painted for the page the window shows now.
    pub(in crate::osr::host) fn fits_page(&self, geometry: SurfaceGeometry) -> bool {
        geometry.size() == self.page_viewport().pixel_size()
    }

    pub(in crate::osr::host) fn main_surface_matches(&self, size: (u32, u32)) -> bool {
        self.main_surface
            .is_some_and(|surface| surface.size() == size)
    }

    pub(in crate::osr::host) fn drive_resize_paint(&mut self) -> Option<Instant> {
        let mut pending = self.pending_resize_paint?;
        if !self.config.visible
            || self.lifecycle_state != LifecycleState::Active
            || self.window.is_none()
            || self.main_surface_matches(pending.size)
        {
            self.pending_resize_paint = None;
            return None;
        }
        let now = Instant::now();
        if now >= pending.deadline {
            self.pending_resize_paint = None;
            if self.presented {
                self.render();
            }
            return None;
        }
        if pending.held_until.is_some_and(|held| now >= held) {
            pending.held_until = None;
            self.pending_resize_paint = Some(pending);
            self.render();
        }
        if now >= pending.retry_at {
            self.retry_resize_paint();
        }
        let pending = self.pending_resize_paint?;
        let wake = pending.retry_at.min(pending.deadline);
        Some(pending.held_until.map_or(wake, |held| held.min(wake)))
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
