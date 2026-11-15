//! The badge and progress every window's taskbar button shows on Windows.
//! Windows opened later start with what the others show.

use std::sync::MutexGuard;

use sabine_platform::{AppBadge, AppProgress};

use super::{BridgeEventEmitter, lock};
use crate::bridge::frame::Frame;
use crate::bridge::writer::BridgeWriter;
use crate::launch::browser::HOST_CONTROL_PREFIX;

#[derive(Default)]
pub(super) struct TaskbarShown {
    badge: Option<String>,
    progress: AppProgress,
}

impl BridgeEventEmitter {
    pub(crate) fn set_badge(&self, badge: Option<&AppBadge>) {
        let mut shown = lock(&self.taskbar);
        let label = badge.and_then(AppBadge::label);
        if label != shown.badge {
            shown.badge = label;
            self.deliver(None, badge_control(shown.badge.as_deref()), None);
        }
    }

    pub(crate) fn set_progress(&self, progress: AppProgress) {
        let mut shown = lock(&self.taskbar);
        if progress != shown.progress {
            shown.progress = progress;
            self.deliver(None, progress_control(progress), None);
        }
    }

    /// Hands a window that attaches what the taskbar buttons show. The
    /// guard holds changes back until the window is attached, so it ends up
    /// with the latest.
    pub(super) fn send_taskbar(&self, writer: &BridgeWriter) -> MutexGuard<'_, TaskbarShown> {
        let shown = lock(&self.taskbar);
        if shown.badge.is_some() {
            writer.send(Frame::shared(&badge_control(shown.badge.as_deref()), None));
        }
        if shown.progress != AppProgress::Hidden {
            writer.send(Frame::shared(&progress_control(shown.progress), None));
        }
        shown
    }
}

fn badge_control(label: Option<&str>) -> String {
    format!(
        "{HOST_CONTROL_PREFIX}\tbadge\t{}",
        serde_json::Value::from(label)
    )
}

fn progress_control(progress: AppProgress) -> String {
    format!(
        "{HOST_CONTROL_PREFIX}\tprogress\t{}",
        crate::desktop::progress_to_json(progress)
    )
}
