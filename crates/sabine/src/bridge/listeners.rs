use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, LazyLock},
    thread,
};

use super::emitter::BridgeEventEmitter;
use crate::WindowId;
use crate::window::{CLOSE_REQUESTED_LINE, WindowListeners};

type Job = Box<dyn FnOnce() + Send>;

/// Hands what a window reports to the app's listeners, which run in the
/// order windows report on a thread of their own, so a listener that takes
/// its time never holds up a window's bridge. A window someone asked to
/// close closes once its listener allows it.
#[derive(Clone)]
pub(super) struct WindowReports {
    window: WindowId,
    listeners: Arc<WindowListeners>,
    emitter: BridgeEventEmitter,
}

impl WindowReports {
    pub(super) fn new(
        window: WindowId,
        listeners: Arc<WindowListeners>,
        emitter: BridgeEventEmitter,
    ) -> Self {
        Self {
            window,
            listeners,
            emitter,
        }
    }

    pub(super) fn report(&self, line: String) {
        let reports = self.clone();
        run_in_order(move || {
            if line != CLOSE_REQUESTED_LINE {
                reports.listeners.deliver(reports.window, &line);
            } else if reports.listeners.allows_close(reports.window) {
                reports.emitter.close_window(reports.window);
            }
        });
    }

    pub(super) fn closed(&self) {
        let reports = self.clone();
        run_in_order(move || reports.listeners.closed(reports.window));
    }
}

/// Runs `job` on the listeners' thread after every job handed to it before.
pub(super) fn run_in_order(job: impl FnOnce() + Send + 'static) {
    static JOBS: LazyLock<crossbeam_channel::Sender<Job>> = LazyLock::new(|| {
        let (jobs, queue) = crossbeam_channel::unbounded::<Job>();
        thread::spawn(move || {
            for job in queue {
                let _ = std::panic::catch_unwind(AssertUnwindSafe(job));
            }
        });
        jobs
    });
    let _ = JOBS.send(Box::new(job));
}
