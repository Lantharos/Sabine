use std::sync::{Arc, Mutex};

use sabine_platform::WindowRegions;

use super::frame::Frame;
use super::writer::BridgeWriter;
use crate::host::WindowId;
use crate::launch::browser::HOST_CONTROL_PREFIX;

/// Sends events to the app's pages. Sending waits while a window has not yet
/// taken earlier events, so a fast producer runs at the pace its pages keep
/// up with instead of losing events.
#[derive(Clone)]
pub struct BridgeEventEmitter {
    targets: Arc<Mutex<Vec<BridgeTarget>>>,
}

struct BridgeTarget {
    window: WindowId,
    writer: BridgeWriter,
}

impl BridgeEventEmitter {
    pub(super) fn new(window: WindowId, writer: BridgeWriter) -> Self {
        Self {
            targets: Arc::new(Mutex::new(vec![BridgeTarget { window, writer }])),
        }
    }

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

    pub(crate) fn attach(&self, window: WindowId, writer: BridgeWriter) {
        if let Ok(mut targets) = self.targets.lock() {
            targets.retain(|target| target.window != window);
            targets.push(BridgeTarget { window, writer });
        }
    }

    pub(crate) fn detach(&self, window: WindowId) {
        if let Ok(mut targets) = self.targets.lock() {
            targets.retain(|target| target.window != window);
        }
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

    pub fn quit(&self) -> bool {
        self.emit_host_control("quit", "1")
    }

    pub fn focus_window(&self) -> bool {
        self.emit_host_control("focus", "1")
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

    pub fn focus_window_with_activation_token(&self, token: Option<&str>) -> bool {
        self.emit_host_control(
            "focus",
            token
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .unwrap_or("1"),
        )
    }

    /// Drive a guest surface from Rust. The payload is forwarded to the CEF
    /// host as `SABINE_HOST_CONTROL\tguest.<op>\t{json}`. Prefer the page
    /// `sabine.guest.*` bridge for UI-driven work; use this for host-owned
    /// guests (for example restoring a session before the page loads).
    pub fn guest_control(&self, control: &sabine_bridge::GuestHostControl) -> bool {
        self.emit_host_control(control.command_name(), &control.to_host_value().to_string())
    }

    pub(crate) fn emit_activity_update(&self, update: &sabine_bridge::ActivityHostUpdate) -> bool {
        let command = match update {
            sabine_bridge::ActivityHostUpdate::Begin(_) => "activity.begin",
            sabine_bridge::ActivityHostUpdate::End(_) => "activity.end",
        };
        self.emit_host_control(
            command,
            &sabine_bridge::host_update_json(update).to_string(),
        )
    }

    pub(crate) fn emit_host_control(&self, command: &str, value: &str) -> bool {
        self.deliver(
            None,
            format!("{HOST_CONTROL_PREFIX}\t{command}\t{value}"),
            None,
        )
    }

    fn deliver(&self, window: Option<WindowId>, line: String, body: Option<&[u8]>) -> bool {
        let writers = match self.targets.lock() {
            Ok(targets) => targets
                .iter()
                .filter(|target| window.is_none_or(|window| window == target.window))
                .map(|target| (target.window, target.writer.clone()))
                .collect::<Vec<_>>(),
            Err(_) => return false,
        };
        let frame: Arc<[u8]> = Frame::encode(&line, body).into();
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

fn regions_control(regions: &WindowRegions) -> String {
    format!(
        "{HOST_CONTROL_PREFIX}\tregions\t{}",
        crate::osr::protocol::regions_to_json(regions)
    )
}

fn event_line(name: &str, payload: &serde_json::Value) -> String {
    let name = serde_json::to_string(name).unwrap_or_else(|_| "\"event\"".to_string());
    format!("SABINE_BRIDGE_EVENT\t{name}\t{payload}")
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::BufRead;
    use std::process::{Child, Command, Stdio};

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
        let emitter = BridgeEventEmitter::new(first.id(), first_writer);
        emitter.attach(second.id(), second_writer);
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
    fn detached_windows_receive_nothing() {
        let (mut child, writer) = window();
        let emitter = BridgeEventEmitter::new(child.id(), writer);
        emitter.detach(child.id());
        assert!(!emitter.emit("ping", serde_json::json!({})));
        let _ = child.kill();
        let _ = child.wait();
    }
}
