mod deadlines;
mod recovery;
pub(super) mod visibility;

use std::time::Instant;

use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::monitor::MonitorHandle;

use crate::osr::protocol::encode_component;

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{
    FALLBACK_ACTIVE_FRAME_RATE, HostActivity, LIFECYCLE_SUSPEND_DEBOUNCE, LifecycleState,
};

impl OsrNativeHost {
    pub(in crate::osr::host) fn send_lifecycle(&self, state: LifecycleState, reason: &str) {
        let (name, frame_rate) = match state {
            LifecycleState::Active => ("active", self.active_frame_rate()),
            LifecycleState::Suspended => (
                "suspended",
                self.config.lifecycle.background_frame_rate.max(1),
            ),
            LifecycleState::Frozen => {
                ("frozen", self.config.lifecycle.background_frame_rate.max(1))
            }
        };
        self.last_frame_rate.set(Some(frame_rate));
        self.send_control(format!(
            "lifecycle\t{name}\t{frame_rate}\t{}\n",
            encode_component(reason)
        ));
        crate::osr::host::trace_host(
            &self.config,
            format!("lifecycle.{name}.{reason}.fps.{frame_rate}"),
        );
    }

    pub(in crate::osr::host) fn active_frame_rate(&self) -> u32 {
        if self.config.lifecycle.active_frame_rate > 0 {
            return self.config.lifecycle.active_frame_rate;
        }
        self.window
            .as_ref()
            .and_then(|window| window.current_monitor())
            .and_then(monitor_frame_rate)
            .unwrap_or(FALLBACK_ACTIVE_FRAME_RATE)
    }

    pub(in crate::osr::host) fn sync_active_frame_rate(&self) {
        if self.lifecycle_state == LifecycleState::Active
            && self.last_frame_rate.get() != Some(self.active_frame_rate())
        {
            self.send_lifecycle(LifecycleState::Active, "monitor");
        }
    }

    fn should_suspend(&self) -> bool {
        !self.config.visible
            || (self.occluded && self.config.lifecycle.suspend_on_occluded)
            || (!self.focused && self.config.lifecycle.suspend_on_blur)
    }

    pub(in crate::osr::host) fn sync_lifecycle(&mut self, reason: &str) {
        if self.closing_deadline.is_some() {
            return;
        }
        if self.should_suspend() {
            self.suspend(reason);
        } else {
            self.resume(reason);
        }
    }

    pub(in crate::osr::host) fn schedule_lifecycle_sync(&mut self, reason: &str) {
        if self.closing_deadline.is_some() {
            return;
        }
        if self.should_suspend() {
            self.pending_suspend_at = Some(Instant::now() + LIFECYCLE_SUSPEND_DEBOUNCE);
            return;
        }
        self.pending_suspend_at = None;
        self.sync_lifecycle(reason);
    }

    pub(in crate::osr::host) fn drive_pending_suspend(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
    ) -> bool {
        let Some(deadline) = self.pending_suspend_at else {
            return false;
        };
        if !self.should_suspend() {
            self.pending_suspend_at = None;
            return false;
        }
        if Instant::now() >= deadline {
            self.pending_suspend_at = None;
            self.suspend("debounced");
            return false;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        true
    }

    pub(in crate::osr::host) fn suspend(&mut self, reason: &str) {
        self.pending_suspend_at = None;
        if self.lifecycle_state == LifecycleState::Active {
            self.lifecycle_state = LifecycleState::Suspended;
            self.send_lifecycle(LifecycleState::Suspended, reason);
        }
        self.refresh_freeze();
    }

    pub(in crate::osr::host) fn resume(&mut self, reason: &str) {
        self.pending_suspend_at = None;
        self.freeze_deadline = None;
        if self.lifecycle_state == LifecycleState::Active {
            return;
        }
        self.lifecycle_state = LifecycleState::Active;
        self.send_lifecycle(LifecycleState::Active, reason);
    }

    pub(in crate::osr::host) fn send_current_lifecycle(&self) {
        self.send_lifecycle(self.lifecycle_state, "connect");
    }

    pub(in crate::osr::host) fn begin_activity(&mut self, activity: HostActivity) {
        if activity.keeps_running {
            self.running_activities.insert(activity.id);
            self.refresh_freeze();
        }
    }

    pub(in crate::osr::host) fn end_activity(&mut self, activity: HostActivity) {
        if activity.keeps_running {
            self.running_activities.remove(&activity.id);
            self.refresh_freeze();
        }
    }

    pub(in crate::osr::host) fn set_page_media_playing(&mut self, playing: bool) {
        self.page_media_playing = playing;
        self.refresh_freeze();
    }

    /// Starts the countdown to freezing a page nobody can see, and thaws a
    /// frozen page as soon as something needs it running.
    pub(in crate::osr::host) fn refresh_freeze(&mut self) {
        let keep_running = !self.running_activities.is_empty()
            || self.page_media_playing
            || self.media.is_playing();
        if keep_running && self.lifecycle_state == LifecycleState::Frozen {
            self.lifecycle_state = LifecycleState::Suspended;
            self.send_lifecycle(LifecycleState::Suspended, "playing");
        }
        let out_of_sight = !self.config.visible || self.occluded;
        let freezable = self.lifecycle_state == LifecycleState::Suspended
            && out_of_sight
            && !keep_running
            && self.socket.is_some();
        match (freezable, self.config.lifecycle.freeze_after) {
            (true, Some(delay)) => {
                self.freeze_deadline
                    .get_or_insert_with(|| Instant::now() + delay);
            }
            _ => self.freeze_deadline = None,
        }
    }

    pub(in crate::osr::host) fn freeze(&mut self) {
        self.freeze_deadline = None;
        if self.lifecycle_state == LifecycleState::Suspended {
            self.lifecycle_state = LifecycleState::Frozen;
            self.send_lifecycle(LifecycleState::Frozen, "idle");
        }
    }
}

fn monitor_frame_rate(monitor: MonitorHandle) -> Option<u32> {
    monitor
        .current_video_mode()?
        .refresh_rate_millihertz()
        .map(|millihertz| millihertz.get().saturating_add(999) / 1000)
}
