use std::sync::{Arc, Mutex, MutexGuard};

use sabine_bridge::{BridgeEvent, Bytes};
use sabine_platform::WindowRegions;

use super::frame::Frame;
use super::writer::BridgeWriter;
use crate::host::WindowId;
use crate::launch::browser::HOST_CONTROL_PREFIX;

/// Sends events to the app's pages. Sending waits while a window has not yet
/// taken earlier events, so a fast producer runs at the pace its pages keep
/// up with instead of losing events.
#[derive(Clone, Default)]
pub struct BridgeEventEmitter {
    targets: Arc<Mutex<Vec<BridgeTarget>>>,
    /// Held while the setting is sent and while windows attach, so every
    /// window ends up with the latest value.
    square_corners: Arc<Mutex<bool>>,
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
        let mut targets = lock(&self.targets);
        targets.retain(|target| target.window != window);
        for activity in activities
            .list()
            .into_iter()
            .filter(|activity| activity.window.is_none())
        {
            let begin = sabine_bridge::ActivityHostUpdate::Begin(activity);
            writer.send(Frame::shared(&activity_control(&begin), None));
        }
        targets.push(BridgeTarget { window, writer });
    }

    pub(crate) fn detach(&self, window: WindowId) {
        lock(&self.targets).retain(|target| target.window != window);
    }

    pub fn set_visible(&self, visible: bool) -> bool {
        self.emit_host_control("visible", if visible { "1" } else { "0" })
    }

    pub fn show(&self) -> bool {
        self.emit_host_control("show", "1")
    }

    pub fn hide(&self) -> bool {
        self.emit_host_control("hide", "1")
    }

    pub(crate) fn quit(&self) -> bool {
        self.emit_host_control("quit", "1")
    }

    pub fn focus_window(&self) -> bool {
        self.emit_host_control("focus", "1")
    }

    /// Closes one window as its close button does, without asking
    /// [`SabineWindow::on_close_requested`](crate::SabineWindow::on_close_requested).
    /// A window built with `hide_on_close` hides instead.
    pub fn close_window(&self, window: WindowId) -> bool {
        self.deliver(
            Some(window),
            format!("{HOST_CONTROL_PREFIX}\tclose\t1"),
            None,
        )
    }

    /// Replaces the blur, opaque and input regions of every window, for
    /// example to drop a sidebar's blur while the sidebar is hidden.
    pub fn set_regions(&self, regions: &WindowRegions) -> bool {
        self.deliver(None, regions_control(regions), None)
    }

    /// Replaces the blur, opaque and input regions of one window.
    pub fn set_regions_of(&self, window: WindowId, regions: &WindowRegions) -> bool {
        self.deliver(Some(window), regions_control(regions), None)
    }

    /// Squares the corners of every window's regions even while it floats,
    /// for desktops where people turned rounded window corners off. Windows
    /// opened later follow it too. Windows that are maximized, fullscreen or
    /// tiled have square corners either way.
    pub fn set_square_corners(&self, square: bool) -> bool {
        let mut current = lock(&self.square_corners);
        *current = square;
        self.deliver(None, square_corners_control(square), None)
    }

    /// Shows a desktop notification from one window. Its pages hear about
    /// clicks through `notification.click` events carrying its id.
    pub fn notify(&self, window: WindowId, notification: &sabine_platform::Notification) -> bool {
        self.deliver(
            Some(window),
            format!(
                "{HOST_CONTROL_PREFIX}\tnotify\t{}",
                crate::osr::protocol::notification_to_json(notification)
            ),
            None,
        )
    }

    /// Prints PDF, image and text files as one document from one window,
    /// through the desktop's print dialog. Pages get the outcome from
    /// `print.files` instead.
    pub fn print_files(
        &self,
        window: WindowId,
        paths: &[std::path::PathBuf],
        title: Option<&str>,
    ) -> bool {
        self.deliver(
            Some(window),
            format!(
                "{HOST_CONTROL_PREFIX}\tprint\t{}",
                serde_json::json!({ "paths": paths, "title": title })
            ),
            None,
        )
    }

    pub fn focus_window_with_activation_token(&self, token: Option<&str>) -> bool {
        self.emit_host_control(
            "focus",
            token
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .unwrap_or("1"),
        )
    }

    /// Drives the guests of one window from Rust, for example to restore a
    /// session's tabs before its page loads. Pages see the results through
    /// guest events; prefer `sabine.guest` for work the page drives.
    pub fn guest_control(
        &self,
        window: WindowId,
        control: &sabine_bridge::GuestHostControl,
    ) -> bool {
        self.deliver(
            Some(window),
            format!(
                "{HOST_CONTROL_PREFIX}\t{}\t{}",
                control.command_name(),
                control.to_host_value()
            ),
            None,
        )
    }

    pub(crate) fn emit_activity_update(&self, update: &sabine_bridge::ActivityHostUpdate) -> bool {
        self.deliver(update.record().window, activity_control(update), None)
    }

    pub(crate) fn emit_host_control(&self, command: &str, value: &str) -> bool {
        self.deliver(
            None,
            format!("{HOST_CONTROL_PREFIX}\t{command}\t{value}"),
            None,
        )
    }

    fn deliver(&self, window: Option<WindowId>, line: String, body: Option<&[u8]>) -> bool {
        let writers = lock(&self.targets)
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

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::BufRead;
    use std::process::{Child, Command, Stdio};

    use sabine_bridge::ActivityRegistry;

    use super::*;

    fn window() -> (Child, BridgeWriter) {
        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("cat");
        let writer = BridgeWriter::spawn(child.stdin.take().expect("stdin"));
        (child, writer)
    }

    fn next_line(child: &mut Child) -> String {
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.as_mut().expect("stdout"))
            .read_line(&mut line)
            .expect("line");
        line
    }

    #[test]
    fn events_reach_only_the_window_they_are_sent_to() {
        let (mut first, first_writer) = window();
        let (mut second, second_writer) = window();
        let emitter = BridgeEventEmitter::default();
        emitter.attach(first.id(), first_writer, &ActivityRegistry::default());
        emitter.attach(second.id(), second_writer, &ActivityRegistry::default());
        assert!(emitter.emit_to(second.id(), "ping", serde_json::json!(2)));
        assert!(emitter.emit("ping", serde_json::json!(1)));
        assert_eq!(next_line(&mut first), "SABINE_BRIDGE_EVENT\t\"ping\"\t1\n");
        assert_eq!(next_line(&mut second), "SABINE_BRIDGE_EVENT\t\"ping\"\t2\n");
        for mut child in [first, second] {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    #[test]
    fn windows_opened_later_get_square_corners() {
        let (mut first, first_writer) = window();
        let emitter = BridgeEventEmitter::default();
        emitter.attach(first.id(), first_writer, &ActivityRegistry::default());
        assert!(emitter.set_square_corners(true));
        let (mut second, second_writer) = window();
        emitter.attach(second.id(), second_writer, &ActivityRegistry::default());
        let control = format!("{HOST_CONTROL_PREFIX}\tsquare-corners\t1\n");
        assert_eq!(next_line(&mut first), control);
        assert_eq!(next_line(&mut second), control);
        for mut child in [first, second] {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    #[test]
    fn windows_opened_later_keep_running_for_the_apps_activities() {
        let activities = ActivityRegistry::default();
        let emitter = BridgeEventEmitter::default();
        let lease = activities.lease(
            sabine_bridge::ActivityOptions::new("sync"),
            Some(Arc::new(emitter.clone())),
        );
        let (mut child, writer) = window();
        emitter.attach(child.id(), writer, &activities);
        let record = lease.record().expect("running activity").clone();
        let begin = activity_control(&sabine_bridge::ActivityHostUpdate::Begin(record));
        assert_eq!(next_line(&mut child), format!("{begin}\n"));
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn detached_windows_receive_nothing() {
        let (mut child, writer) = window();
        let emitter = BridgeEventEmitter::default();
        emitter.attach(child.id(), writer, &ActivityRegistry::default());
        emitter.detach(child.id());
        assert!(!emitter.emit("ping", serde_json::json!({})));
        let _ = child.kill();
        let _ = child.wait();
    }
}
