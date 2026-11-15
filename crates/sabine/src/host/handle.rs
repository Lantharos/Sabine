use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[cfg(target_os = "linux")]
use std::time::Duration;

use sabine_platform::{AppBadge, AppProgress, PowerState, PowerStatus};

use super::page_window::PageWindow;
use super::process::{
    PRIMARY_READY_TIMEOUT, PendingWindow, ProcessCommand, WindowId, WindowRequest,
};
use crate::desktop::{Autostart, Launcher};
use crate::{SabineResult, SabineWindow};

/// How long a launch or the installer waits for the app to answer whether it
/// steps down for its newly installed copy.
#[cfg(target_os = "linux")]
const STEP_DOWN_ANSWER_TIMEOUT: Duration = Duration::from_secs(2);

/// What app code holds on to for driving the app's process from any thread,
/// also while [`SabineProcess::wait`](crate::SabineProcess::wait) runs.
#[derive(Clone)]
pub struct SabineProcessHandle {
    pub(super) command_sender: crossbeam_channel::Sender<ProcessCommand>,
    autostart: Autostart,
    launcher: Launcher,
    power: PowerStatus,
}

impl SabineProcessHandle {
    /// A handle and the queue its commands reach the process on.
    pub(crate) fn new(
        autostart: Autostart,
        launcher: Launcher,
        power: PowerStatus,
    ) -> (Self, crossbeam_channel::Receiver<ProcessCommand>) {
        let (command_sender, commands) = crossbeam_channel::unbounded();
        (
            Self {
                command_sender,
                autostart,
                launcher,
                power,
            },
            commands,
        )
    }

    /// Opens another window, waiting up to 20 seconds for the process to
    /// open it. A window that isn't open by then never opens.
    pub fn open_window(&self, window: SabineWindow) -> SabineResult<WindowId> {
        self.request_window(WindowRequest::Built(Box::new(window)))
    }

    pub(crate) fn open_page_window(&self, page: PageWindow) -> SabineResult<WindowId> {
        self.request_window(WindowRequest::Page(page))
    }

    /// Closes one of the app's windows as its close button does. Returns
    /// whether the app has a window with that id.
    pub(crate) fn close_window(&self, window: WindowId) -> bool {
        let (closed, answer) = crossbeam_channel::bounded(1);
        self.command_sender
            .send(ProcessCommand::CloseWindow { window, closed })
            .is_ok()
            && answer.recv().unwrap_or(false)
    }

    fn request_window(&self, request: WindowRequest) -> SabineResult<WindowId> {
        let (response, result) = crossbeam_channel::bounded(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        self.command_sender
            .send(ProcessCommand::OpenWindow(PendingWindow {
                request,
                response,
                cancelled: Arc::clone(&cancelled),
            }))
            .map_err(|_| crate::SabineError::CreationFailed {
                message: "Sabine process is no longer running".into(),
            })?;
        result.recv_timeout(PRIMARY_READY_TIMEOUT).map_err(|_| {
            cancelled.store(true, Ordering::SeqCst);
            crate::SabineError::CreationFailed {
                message: "Sabine process did not open the window in time".into(),
            }
        })?
    }

    /// Shows and focuses the app's window, opening it first while a
    /// background app runs without one.
    pub fn present(&self, activation_token: Option<String>) -> bool {
        self.command_sender
            .send(ProcessCommand::Present { activation_token })
            .is_ok()
    }

    /// Closes every window and ends the app, also one that runs in the
    /// background or hides its window when it closes.
    pub fn quit(&self) -> bool {
        self.command_sender.send(ProcessCommand::Quit).is_ok()
    }

    /// Asks the app whether it steps down for its newly installed copy, as a
    /// `launch` of that copy or its installer asks.
    #[cfg(target_os = "linux")]
    pub(crate) fn step_down(&self, launch: bool) -> bool {
        let (answer, answered) = crossbeam_channel::bounded(1);
        self.command_sender
            .send(ProcessCommand::StepDown { launch, answer })
            .is_ok()
            && answered
                .recv_timeout(STEP_DOWN_ANSWER_TIMEOUT)
                .unwrap_or(false)
    }

    /// Turns the app's [`SabineWindow::autostart`] entry on or off, as an
    /// in-app setting would. Turning it on doesn't override people who turned
    /// the app off in their system settings. Development runs change nothing.
    pub fn set_autostart(&self, enabled: bool) -> Result<(), String> {
        self.autostart.set_enabled(enabled)
    }

    /// Shows a badge on the app's Dock or taskbar icon, such as the number of
    /// unread messages, or takes it off with `None`.
    pub fn set_badge(&self, badge: Option<AppBadge>) {
        self.launcher.set_badge(badge);
    }

    /// Shows a progress bar on the app's Dock or taskbar icon, or hides it
    /// with [`AppProgress::Hidden`].
    pub fn set_progress(&self, progress: AppProgress) {
        self.launcher.set_progress(progress);
    }

    /// Adds a file the person opened to the desktop's list of recent files,
    /// which the app's jump list, Dock menu and file choosers show.
    pub fn add_recent_document(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        self.launcher.add_recent_document(path.as_ref())
    }

    /// Takes the files the app added off the desktop's list of recent files.
    pub fn clear_recent_documents(&self) -> Result<(), String> {
        self.launcher.clear_recent_documents()
    }

    /// The power source and screen lock as the desktop last reported them.
    /// Changes arrive as [`PlatformEvent::Power`](sabine_platform::PlatformEvent::Power).
    pub fn power_state(&self) -> PowerState {
        self.power.get()
    }
}
