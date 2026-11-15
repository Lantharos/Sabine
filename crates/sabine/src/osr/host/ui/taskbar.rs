use std::sync::{Arc, Weak};

use sabine_platform::{AppBadge, AppProgress, Taskbar};
use winit::window::Window;

/// The badge and progress the app shows on the window's taskbar button. The
/// button is followed once there is something to show.
#[derive(Default)]
pub(in crate::osr::host) struct TaskbarButton {
    /// The window until its button is followed, which the host may close
    /// before then.
    window: Option<Weak<dyn Window>>,
    taskbar: Option<Taskbar>,
    badge: Option<AppBadge>,
    progress: AppProgress,
}

impl TaskbarButton {
    pub(in crate::osr::host) fn attach(&mut self, window: &Arc<dyn Window>) {
        self.taskbar = None;
        self.window = Some(Arc::downgrade(window));
        if self.badge.is_some() || self.progress != AppProgress::Hidden {
            self.show();
        }
    }

    pub(in crate::osr::host) fn set_badge(&mut self, label: Option<String>) {
        let badge = label.map(AppBadge::Text);
        if badge != self.badge {
            self.badge = badge;
            self.show();
        }
    }

    pub(in crate::osr::host) fn set_progress(&mut self, progress: AppProgress) {
        if progress != self.progress {
            self.progress = progress;
            self.show();
        }
    }

    /// A window whose button can't be followed shows nothing, and says so
    /// once.
    fn show(&mut self) {
        if self.taskbar.is_none()
            && let Some(window) = self.window.take().and_then(|window| window.upgrade())
        {
            match Taskbar::new(window.as_ref()) {
                Ok(taskbar) => self.taskbar = Some(taskbar),
                Err(error) => sabine_runtime::report_error("window", error),
            }
        }
        if let Some(taskbar) = &self.taskbar {
            taskbar.set_badge(self.badge.as_ref());
            taskbar.set_progress(self.progress);
        }
    }
}
