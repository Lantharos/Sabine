mod dialogs;
mod menus;
mod page;
#[cfg(target_os = "windows")]
mod taskbar;
#[cfg(all(test, unix))]
mod tests;
mod windows;

pub(crate) use dialogs::DIALOG_ANSWER_LINE;
pub use dialogs::{DialogAnswer, DialogInput, DialogReply, MessageDialog};
pub(crate) use menus::APP_MENU_LINE;

use std::sync::{Arc, Mutex, MutexGuard};

use sabine_bridge::{BridgeEvent, Bytes};
use sabine_platform::{Display, WindowRegions};

use super::frame::Frame;
use super::writer::BridgeWriter;
use crate::host::WindowId;
use crate::launch::browser::HOST_CONTROL_PREFIX;

/// Sends events to the app's pages. Sending waits while a window has not yet
/// taken earlier events, so a fast producer runs at the pace its pages keep
/// up with instead of losing events.
#[derive(Clone, Default)]
pub struct BridgeEventEmitter {
    targets: Arc<Mutex<Targets>>,
    /// Held while the setting is sent and while windows attach, so every
    /// window ends up with the latest value.
    square_corners: Arc<Mutex<bool>>,
    /// The displays as the app's windows last reported them.
    displays: Arc<Mutex<Vec<Display>>>,
    #[cfg(target_os = "windows")]
    taskbar: Arc<Mutex<taskbar::TaskbarShown>>,
    /// The app menu bar's control, held the same way.
    app_menu: Arc<Mutex<Option<String>>>,
    dialogs: Arc<Mutex<dialogs::PendingDialogs>>,
}

#[derive(Default)]
struct Targets {
    windows: Vec<BridgeTarget>,
    focused: Option<WindowId>,
}

impl Targets {
    /// The window calls for the app's window act on: the one focused last,
    /// or else the first one open.
    fn front(&self) -> Option<WindowId> {
        self.focused
            .or_else(|| self.windows.first().map(|target| target.window))
    }
}

struct BridgeTarget {
    window: WindowId,
    writer: BridgeWriter,
}

impl BridgeEventEmitter {
    /// Sends an event to every window.
    pub fn emit(&self, name: impl Into<String>, payload: serde_json::Value) -> bool {
        self.deliver(None, event_line(&name.into(), &payload), None)
    }

    /// Sends an event to one window.
    pub fn emit_to(
        &self,
        window: WindowId,
        name: impl Into<String>,
        payload: serde_json::Value,
    ) -> bool {
        self.deliver(Some(window), event_line(&name.into(), &payload), None)
    }

    /// Sends a typed event to every window. Returns `false` when the payload
    /// does not serialize or no window takes it.
    pub fn send<T: serde::Serialize>(&self, event: &BridgeEvent<T>, payload: &T) -> bool {
        serde_json::to_value(payload).is_ok_and(|payload| self.emit(event.name(), payload))
    }

    /// Sends a typed bytes event to every window.
    pub fn send_bytes(&self, event: &BridgeEvent<Bytes>, bytes: &[u8]) -> bool {
        self.emit_bytes(event.name(), bytes)
    }

    /// Sends bytes to every window; listeners receive a `Uint8Array`. Use this
    /// for streams such as terminal output.
    pub fn emit_bytes(&self, name: impl Into<String>, bytes: &[u8]) -> bool {
        self.deliver(
            None,
            event_line(&name.into(), &serde_json::Value::Null),
            Some(bytes),
        )
    }

    /// Sends bytes to one window; listeners receive a `Uint8Array`.
    pub fn emit_bytes_to(&self, window: WindowId, name: impl Into<String>, bytes: &[u8]) -> bool {
        self.deliver(
            Some(window),
            event_line(&name.into(), &serde_json::Value::Null),
            Some(bytes),
        )
    }

    /// Starts sending a window events, beginning with the app's settings and
    /// the activities the app runs, so it keeps running for them as the
    /// windows opened before it do. The windows are held while the activities
    /// are sent, so an activity that ends meanwhile ends after it began.
    pub(crate) fn attach(
        &self,
        window: WindowId,
        writer: BridgeWriter,
        activities: &sabine_bridge::ActivityRegistry,
    ) {
        let square_corners = lock(&self.square_corners);
        if *square_corners {
            writer.send(Frame::shared(&square_corners_control(true), None));
        }
        #[cfg(target_os = "windows")]
        let _taskbar = self.send_taskbar(&writer);
        let app_menu = lock(&self.app_menu);
        if let Some(control) = app_menu.as_deref() {
            writer.send(Frame::shared(control, None));
        }
        let mut targets = lock(&self.targets);
        targets.windows.retain(|target| target.window != window);
        for activity in activities
            .list()
            .into_iter()
            .filter(|activity| activity.window.is_none())
        {
            let begin = sabine_bridge::ActivityHostUpdate::Begin(activity);
            writer.send(Frame::shared(&activity_control(&begin), None));
        }
        targets.windows.push(BridgeTarget { window, writer });
    }

    pub(crate) fn detach(&self, window: WindowId) {
        lock(&self.dialogs).window_closed(window);
        let mut targets = lock(&self.targets);
        targets.windows.retain(|target| target.window != window);
        if targets.focused == Some(window) {
            targets.focused = None;
        }
    }

    /// Notes the window people moved focus to, which calls for the app's
    /// window act on from then on.
    pub(crate) fn focused(&self, window: WindowId) {
        let mut targets = lock(&self.targets);
        if targets.windows.iter().any(|target| target.window == window) {
            targets.focused = Some(window);
        }
    }

    pub(crate) fn emit_activity_update(&self, update: &sabine_bridge::ActivityHostUpdate) -> bool {
        self.deliver(update.record().window, activity_control(update), None)
    }

    pub(super) fn host_control(
        &self,
        window: Option<WindowId>,
        command: &str,
        value: &str,
    ) -> bool {
        self.deliver(
            window,
            format!("{HOST_CONTROL_PREFIX}\t{command}\t{value}"),
            None,
        )
    }

    /// Sends a line to every window but one.
    pub(super) fn deliver_to_others(&self, window: WindowId, line: &str) {
        let others = lock(&self.targets)
            .windows
            .iter()
            .map(|target| target.window)
            .filter(|other| *other != window)
            .collect::<Vec<_>>();
        for other in others {
            self.deliver(Some(other), line.to_string(), None);
        }
    }

    fn front_control(&self, command: &str, value: &str) -> bool {
        let front = lock(&self.targets).front();
        front.is_some_and(|window| self.host_control(Some(window), command, value))
    }

    pub(super) fn deliver(
        &self,
        window: Option<WindowId>,
        line: String,
        body: Option<&[u8]>,
    ) -> bool {
        let writers = lock(&self.targets)
            .windows
            .iter()
            .filter(|target| window.is_none_or(|window| window == target.window))
            .map(|target| (target.window, target.writer.clone()))
            .collect::<Vec<_>>();
        let frame = Frame::shared(&line, body);
        let mut delivered = false;
        for (window, writer) in writers {
            if writer.send(Arc::clone(&frame)) {
                delivered = true;
            } else {
                self.detach(window);
            }
        }
        delivered
    }
}

impl sabine_bridge::ActivityEventEmitter for BridgeEventEmitter {
    fn emit_activity_update(&self, update: &sabine_bridge::ActivityHostUpdate) -> bool {
        BridgeEventEmitter::emit_activity_update(self, update)
    }
}

fn activity_control(update: &sabine_bridge::ActivityHostUpdate) -> String {
    let command = match update {
        sabine_bridge::ActivityHostUpdate::Begin(_) => "activity.begin",
        sabine_bridge::ActivityHostUpdate::End(_) => "activity.end",
    };
    format!(
        "{HOST_CONTROL_PREFIX}\t{command}\t{}",
        sabine_bridge::host_update_json(update)
    )
}

fn regions_control(regions: &WindowRegions) -> String {
    format!(
        "{HOST_CONTROL_PREFIX}\tregions\t{}",
        crate::osr::protocol::regions_to_json(regions)
    )
}

fn square_corners_control(square: bool) -> String {
    format!(
        "{HOST_CONTROL_PREFIX}\tsquare-corners\t{}",
        u8::from(square)
    )
}

pub(super) fn event_line(name: &str, payload: &serde_json::Value) -> String {
    format!(
        "SABINE_BRIDGE_EVENT\t{}\t{payload}",
        serde_json::Value::from(name)
    )
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}
