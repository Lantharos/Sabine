use std::{
    process::ExitStatus,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use sabine_bridge::{ActivityOptions, ActivityRecord, SabineActivityLease};

use super::SabineProcessHandle;
use crate::bridge::BridgeEventEmitter;
use crate::desktop::DesktopServiceState;
use crate::host::process_tree::ManagedChild;
use crate::launch::metrics::{LaunchMetrics, SabineLaunchMetricsSnapshot};
use crate::osr::launch::WindowContext;
use crate::{SabineResult, SabineWindow};

/// How long opening another window waits for the app's first window, whose
/// browser the others join.
pub(super) const PRIMARY_READY_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) enum ProcessCommand {
    OpenWindow(PendingWindow),
    Present {
        activation_token: Option<String>,
    },
    PrimaryReady {
        window: WindowId,
        ready: bool,
    },
    Quit,
    /// A launch of the app's newly installed copy, or its installer, asks the
    /// app to end so that copy runs instead.
    #[cfg(target_os = "linux")]
    StepDown {
        launch: bool,
        answer: crossbeam_channel::Sender<bool>,
    },
}

/// A window to open once the app's first window is ready, unless whoever
/// asked for it stopped waiting.
pub(crate) struct PendingWindow {
    pub(super) window: Box<SabineWindow>,
    pub(super) response: crossbeam_channel::Sender<SabineResult<WindowId>>,
    pub(super) cancelled: Arc<AtomicBool>,
}

/// The window the app opens with, and opens again when a background app is
/// activated after its window closed.
pub(crate) struct PrimaryWindow {
    pub(crate) child: ManagedChild,
    pub(crate) ready: crossbeam_channel::Receiver<()>,
    pub(crate) is_ready: bool,
}

pub struct SabineProcess {
    pub(crate) _runtime_lease: sabine_runtime::RuntimeLease,
    pub(crate) primary: Option<PrimaryWindow>,
    pub(crate) primary_status: Option<ExitStatus>,
    pub(crate) background: bool,
    pub(crate) extra_windows: Vec<ManagedChild>,
    pub(crate) pending_windows: Vec<PendingWindow>,
    pub(crate) child_exit_sender: crossbeam_channel::Sender<u32>,
    pub(crate) child_exit_receiver: crossbeam_channel::Receiver<u32>,
    pub(crate) command_receiver: crossbeam_channel::Receiver<ProcessCommand>,
    pub(crate) handle: SabineProcessHandle,
    pub(crate) quitting: bool,
    pub(crate) bridge_threads: Vec<JoinHandle<()>>,
    pub(crate) bridge_emitter: BridgeEventEmitter,
    pub(crate) desktop_services: Option<DesktopServiceState>,
    pub(crate) open_urls: crate::desktop::open_urls::OpenUrls,
    pub(crate) desktop_event_thread: Option<JoinHandle<()>>,
    pub(crate) desktop_event_stop: Option<crossbeam_channel::Sender<()>>,
    pub(crate) activity: sabine_bridge::ActivityRegistry,
    pub(crate) metrics: LaunchMetrics,
    pub(crate) windows: WindowContext,
    #[cfg(target_os = "linux")]
    pub(crate) installation: Option<crate::host::Installation>,
}

/// Identifier for a window opened via [`SabineProcess::open_window`].
/// Matches the OSR-host child process id.
pub type WindowId = u32;

impl SabineProcess {
    /// Consumes URLs queued by the initial launch, another instance, or the OS.
    pub fn take_open_urls(&self) -> Vec<String> {
        self.open_urls.take()
    }

    /// The id of the window the app opened with, while it is open.
    pub fn id(&self) -> Option<WindowId> {
        self.primary.as_ref().map(|primary| primary.child.id())
    }

    pub fn handle(&self) -> SabineProcessHandle {
        self.handle.clone()
    }

    /// Open another OSR window in this process. It shares the `app_id` and
    /// bridge handlers of the window the app opened with, and adds the
    /// handlers registered on `window`, which answer in its place for a
    /// command both register.
    pub fn open_window(&mut self, window: SabineWindow) -> SabineResult<WindowId> {
        if let Some(primary) = &mut self.primary
            && !primary.is_ready
        {
            primary
                .ready
                .recv_timeout(PRIMARY_READY_TIMEOUT)
                .map_err(|_| primary_not_ready())?;
            primary.is_ready = true;
        }
        self.attach_window(window)
    }

    fn attach_window(&mut self, window: SabineWindow) -> SabineResult<WindowId> {
        let (config, handlers, url) = window.into_open_window_parts()?;
        crate::osr::launch::attach_open_window(self, &config, handlers, &url)
    }

    /// Opens a window asked for while the process waits, without blocking
    /// the wait: until the first window is ready, it waits in line.
    fn open_when_ready(&mut self, pending: PendingWindow) {
        let waiting = !self.pending_windows.is_empty();
        let starting = match self.primary.as_mut() {
            Some(primary) if !primary.is_ready && !waiting => match primary.ready.try_recv() {
                Ok(()) => {
                    primary.is_ready = true;
                    false
                }
                Err(crossbeam_channel::TryRecvError::Empty) => true,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    let _ = pending.response.send(Err(primary_not_ready()));
                    return;
                }
            },
            Some(primary) => !primary.is_ready,
            None => false,
        };
        if starting {
            if !waiting {
                self.watch_primary_ready();
            }
            self.pending_windows.push(pending);
            return;
        }
        self.open_pending(pending);
    }

    /// Opens the window, and closes it again when whoever asked for it
    /// stopped waiting meanwhile.
    fn open_pending(&mut self, pending: PendingWindow) {
        if pending.cancelled.load(Ordering::SeqCst) {
            return;
        }
        let opened = self.attach_window(*pending.window);
        if let Err(unanswered) = pending.response.send(opened)
            && let Ok(window) = unanswered.into_inner()
        {
            self.close_window(window);
        }
    }

    fn watch_primary_ready(&self) {
        let Some(primary) = &self.primary else {
            return;
        };
        let window = primary.child.id();
        let ready = primary.ready.clone();
        let commands = self.handle.command_sender.clone();
        thread::spawn(move || {
            let ready = ready.recv_timeout(PRIMARY_READY_TIMEOUT).is_ok();
            let _ = commands.send(ProcessCommand::PrimaryReady { window, ready });
        });
    }

    /// Opens the windows waiting for the first window, or fails them when it
    /// closed or never became ready.
    fn primary_ready(&mut self, window: WindowId, ready: bool) {
        if let Some(primary) = self
            .primary
            .as_mut()
            .filter(|primary| primary.child.id() == window)
        {
            primary.is_ready = ready;
        }
        for pending in std::mem::take(&mut self.pending_windows) {
            if ready {
                self.open_when_ready(pending);
            } else {
                let _ = pending.response.send(Err(primary_not_ready()));
            }
        }
    }

    /// Close one OSR window. Returns `false` if the id is unknown. Closing the
    /// last remaining window leaves `wait()` free to return.
    pub fn close_window(&mut self, window_id: WindowId) -> bool {
        if let Some(mut primary) = self
            .primary
            .take_if(|primary| primary.child.id() == window_id)
        {
            self.bridge_emitter.detach(window_id);
            self.primary_status = primary.child.terminate();
            return true;
        }
        if let Some(index) = self
            .extra_windows
            .iter()
            .position(|window| window.id() == window_id)
        {
            let mut window = self.extra_windows.remove(index);
            self.bridge_emitter.detach(window_id);
            let _ = window.terminate();
            true
        } else {
            false
        }
    }

    /// Block until every OSR window has exited, then tear down the bridge. A
    /// background app keeps waiting for its next activation instead, until
    /// [`SabineProcessHandle::quit`] ends it.
    pub fn wait(mut self) -> std::io::Result<ExitStatus> {
        #[cfg(target_os = "macos")]
        crate::desktop::desktop_wait::wait(&mut self)?;
        #[cfg(not(target_os = "macos"))]
        while !self.collect_exited_windows()? {
            crossbeam_channel::select! {
                recv(self.child_exit_receiver) -> _ => {}
                recv(self.command_receiver) -> command => {
                    if let Ok(command) = command {
                        self.handle_command(command);
                    }
                }
            }
        }

        if let Some(listener) = &self.windows.primary.quit {
            listener.deliver();
        }
        self.stop_desktop_event_forwarder();
        self.join_bridge_threads();
        if self.quitting {
            return Ok(ExitStatus::default());
        }
        self.primary_status.ok_or_else(|| {
            std::io::Error::other("Sabine process exited without a primary window status")
        })
    }

    pub(crate) fn collect_exited_windows(&mut self) -> std::io::Result<bool> {
        if let Some(primary) = &mut self.primary
            && let Some(status) = primary.child.try_wait()?
        {
            self.bridge_emitter.detach(primary.child.id());
            self.primary_status = Some(status);
            self.primary = None;
        }

        let emitter = &self.bridge_emitter;
        self.extra_windows
            .retain_mut(|window| match window.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) | Err(_) => {
                    emitter.detach(window.id());
                    false
                }
            });

        Ok((self.quitting || !self.background)
            && self.primary.is_none()
            && self.extra_windows.is_empty())
    }

    /// Carries out what app code and pages asked of the process. Once the app
    /// quits, it opens and shows nothing more.
    pub(crate) fn handle_command(&mut self, command: ProcessCommand) {
        match command {
            #[cfg(target_os = "linux")]
            ProcessCommand::StepDown { launch, answer } => self.step_down(launch, answer),
            _ if self.quitting => {}
            ProcessCommand::OpenWindow(pending) => self.open_when_ready(pending),
            ProcessCommand::Present { activation_token } => self.present(activation_token),
            ProcessCommand::PrimaryReady { window, ready } => self.primary_ready(window, ready),
            ProcessCommand::Quit => self.quit(),
        }
    }

    fn quit(&mut self) {
        self.quitting = true;
        self.pending_windows.clear();
        self.bridge_emitter.quit();
    }

    /// Ends a background app with no window open once a newer copy of it is
    /// installed, for that copy to take over. A launch that asks while the
    /// app quits anyway takes over once it ended.
    #[cfg(target_os = "linux")]
    fn step_down(&mut self, launch: bool, answer: crossbeam_channel::Sender<bool>) {
        let steps_down = if self.quitting {
            launch
        } else {
            self.background
                && self.primary.is_none()
                && self.extra_windows.is_empty()
                && self
                    .installation
                    .as_ref()
                    .is_some_and(crate::host::Installation::superseded)
        };
        if answer.send(steps_down).is_ok() && steps_down && !self.quitting {
            self.quit();
        }
    }

    fn present(&mut self, activation_token: Option<String>) {
        if self.background && self.primary.is_none() {
            if let Err(error) = crate::osr::launch::open_primary_window(self, activation_token) {
                sabine_runtime::report_error("window", error);
            }
            return;
        }
        self.bridge_emitter.show();
        self.bridge_emitter
            .focus_window_with_activation_token(activation_token.as_deref());
    }

    /// The app's tray icon, to change while it runs.
    pub fn tray(&self) -> Option<crate::TrayHandle> {
        self.desktop_services
            .as_ref()
            .and_then(DesktopServiceState::tray)
    }

    /// Drives the guests of the app's first window from Rust. See
    /// [`BridgeEventEmitter::guest_control`].
    pub fn guest_control(&self, control: &sabine_bridge::GuestHostControl) -> bool {
        self.id()
            .is_some_and(|window| self.bridge_emitter.guest_control(window, control))
    }

    pub fn set_visible(&self, visible: bool) -> bool {
        self.bridge_emitter.set_visible(visible)
    }

    pub fn show(&self) -> bool {
        self.bridge_emitter.show()
    }

    pub fn hide(&self) -> bool {
        self.bridge_emitter.hide()
    }

    /// Shows a desktop notification from the app's first window.
    pub fn notify(&self, notification: &sabine_platform::Notification) -> bool {
        self.id()
            .is_some_and(|window| self.bridge_emitter.notify(window, notification))
    }

    pub fn focus_window(&self) -> bool {
        self.bridge_emitter.focus_window()
    }

    pub fn begin_activity(&self, name: impl Into<String>) -> SabineActivityLease {
        self.begin_activity_with(ActivityOptions::new(name))
    }

    pub fn begin_activity_with(&self, options: ActivityOptions) -> SabineActivityLease {
        let emitter = Arc::new(self.bridge_emitter.clone())
            as Arc<dyn sabine_bridge::ActivityEventEmitter>;
        self.activity.lease(options, Some(emitter))
    }

    pub fn activities(&self) -> Vec<ActivityRecord> {
        self.activity.list()
    }

    pub fn bridge_event_emitter(&self) -> BridgeEventEmitter {
        self.bridge_emitter.clone()
    }

    pub fn metrics(&self) -> SabineLaunchMetricsSnapshot {
        self.metrics.snapshot()
    }

    fn cleanup_extra_windows(&mut self) {
        for window in &mut self.extra_windows {
            self.bridge_emitter.detach(window.id());
            let _ = window.terminate();
        }
        self.extra_windows.clear();
    }

    fn join_bridge_threads(&mut self) {
        for thread in self.bridge_threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn primary_not_ready() -> crate::SabineError {
    crate::SabineError::CreationFailed {
        message: "primary Sabine window did not become ready before opening another window".into(),
    }
}

impl Drop for SabineProcess {
    fn drop(&mut self) {
        self.cleanup_extra_windows();
        self.stop_desktop_event_forwarder();
        if let Some(mut primary) = self.primary.take() {
            let _ = primary.child.terminate();
        }
        self.join_bridge_threads();
    }
}
