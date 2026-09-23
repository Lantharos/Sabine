use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use sabine_bridge::BridgeHandlers;
use sabine_platform::PlatformEvent;

use super::request_dispatch::{BridgeIpcRequest, BridgeRequestDispatcher};
use crate::launch::browser::HOST_CONTROL_PREFIX;

#[derive(Clone)]
pub struct BridgeEventEmitter {
    targets: Arc<Mutex<Vec<BridgeTarget>>>,
}

struct BridgeTarget {
    window_id: u32,
    writer: BridgeWriter,
}

#[derive(Clone)]
pub(crate) struct BridgeWriter {
    sender: crossbeam_channel::Sender<String>,
}

impl BridgeWriter {
    fn spawn(mut stdin: std::process::ChildStdin) -> Self {
        let (sender, receiver) = crossbeam_channel::bounded::<String>(512);
        thread::spawn(move || {
            while let Ok(line) = receiver.recv() {
                if writeln!(stdin, "{line}").is_err() || stdin.flush().is_err() {
                    break;
                }
            }
        });
        Self { sender }
    }

    pub(super) fn try_send(
        &self,
        line: impl Into<String>,
    ) -> Result<(), crossbeam_channel::TrySendError<String>> {
        self.sender.try_send(line.into())
    }

    pub(super) fn send(&self, line: impl Into<String>) -> bool {
        self.sender.send(line.into()).is_ok()
    }
}

impl BridgeEventEmitter {
    pub fn emit(&self, name: impl Into<String>, payload: serde_json::Value) -> bool {
        let event = BridgeIpcEvent {
            name: name.into(),
            payload,
        };
        self.write_line(event)
    }

    pub(crate) fn attach(&self, window_id: u32, writer: BridgeWriter) {
        if let Ok(mut targets) = self.targets.lock() {
            targets.retain(|target| target.window_id != window_id);
            targets.push(BridgeTarget { window_id, writer });
        }
    }

    pub(crate) fn detach(&self, window_id: u32) {
        if let Ok(mut targets) = self.targets.lock() {
            targets.retain(|target| target.window_id != window_id);
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
        self.write_line(format!("{HOST_CONTROL_PREFIX}\t{command}\t{value}"))
    }

    fn write_line(&self, line: impl std::fmt::Display) -> bool {
        let Ok(mut targets) = self.targets.lock() else {
            return false;
        };
        if targets.is_empty() {
            return false;
        }
        let message = line.to_string();
        let mut delivered = false;
        targets.retain(|target| match target.writer.try_send(message.clone()) {
            Ok(()) => {
                delivered = true;
                true
            }
            Err(crossbeam_channel::TrySendError::Full(_)) => true,
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => false,
        });
        delivered
    }
}

impl sabine_bridge::ActivityEventEmitter for BridgeEventEmitter {
    fn emit_activity_update(&self, update: &sabine_bridge::ActivityHostUpdate) -> bool {
        BridgeEventEmitter::emit_activity_update(self, update)
    }
}

pub(crate) fn platform_event_payload(event: PlatformEvent) -> (&'static str, serde_json::Value) {
    match event {
        PlatformEvent::OpenUrls(_) => ("app.openUrlsAvailable", serde_json::Value::Null),
        PlatformEvent::Tray(activation) => (
            "tray.activate",
            serde_json::json!({
                "trayId": activation.tray_id,
                "itemId": activation.item_id,
                "action": activation.action,
            }),
        ),
        PlatformEvent::GlobalShortcut(activation) => (
            "globalShortcut.activate",
            serde_json::json!({
                "id": activation.id,
                "action": activation.action,
                "activationToken": activation.activation_token,
            }),
        ),
        PlatformEvent::SingleInstance(activation) => (
            "singleInstance.activate",
            serde_json::json!({
                "policy": format!("{:?}", activation.policy),
                "arguments": activation.arguments,
                "workingDirectory": activation.working_directory,
                "activationToken": activation.activation_token,
            }),
        ),
    }
}

pub(crate) fn prepare_bridge_command(command: &mut Command, _bridge_handlers: &BridgeHandlers) {
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
}

pub(crate) struct BridgeDispatch {
    pub(crate) thread: Option<JoinHandle<()>>,
    pub(crate) emitter: Option<BridgeEventEmitter>,
    pub(crate) ready: crossbeam_channel::Receiver<()>,
}

pub(crate) fn spawn_bridge_dispatch(
    child: &mut Child,
    bridge_runtime: sabine_bridge::BridgeRuntime,
    activity: sabine_bridge::ActivityRegistry,
) -> BridgeDispatch {
    let (ready_sender, ready) = crossbeam_channel::bounded(1);
    let Some(stdin) = child.stdin.take() else {
        return BridgeDispatch {
            thread: None,
            emitter: None,
            ready,
        };
    };
    let writer = BridgeWriter::spawn(stdin);
    let window_id = child.id();
    let emitter = BridgeEventEmitter {
        targets: Arc::new(Mutex::new(vec![BridgeTarget {
            window_id,
            writer: writer.clone(),
        }])),
    };
    let Some(stdout) = child.stdout.take() else {
        return BridgeDispatch {
            thread: None,
            emitter: Some(emitter),
            ready,
        };
    };
    let dispatcher =
        BridgeRequestDispatcher::new(bridge_runtime, activity, emitter.clone(), writer);
    let detach_emitter = emitter.clone();
    let thread = thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(std::result::Result::ok) {
            if line == "SABINE_OSR_READY" {
                let _ = ready_sender.try_send(());
                continue;
            }
            let Some(request) = BridgeIpcRequest::parse(&line) else {
                continue;
            };
            dispatcher.submit(request);
        }
        detach_emitter.detach(window_id);
    });
    BridgeDispatch {
        thread: Some(thread),
        emitter: Some(emitter),
        ready,
    }
}

/// Attach another OSR-host child to an existing bridge emitter and dispatch
/// that window's bridge requests through the same handlers.
pub(crate) fn spawn_bridge_dispatch_for_window(
    child: &mut Child,
    bridge_runtime: sabine_bridge::BridgeRuntime,
    activity: sabine_bridge::ActivityRegistry,
    emitter: &BridgeEventEmitter,
) -> Option<JoinHandle<()>> {
    let writer = BridgeWriter::spawn(child.stdin.take()?);
    let window_id = child.id();
    emitter.attach(window_id, writer.clone());
    let stdout = child.stdout.take()?;
    let activity_emitter = emitter.clone();
    let dispatcher =
        BridgeRequestDispatcher::new(bridge_runtime, activity, emitter.clone(), writer);
    Some(thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(std::result::Result::ok) {
            let Some(request) = BridgeIpcRequest::parse(&line) else {
                continue;
            };
            dispatcher.submit(request);
        }
        activity_emitter.detach(window_id);
    }))
}

pub(crate) fn parse_host_control(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(3, '\t');
    if parts.next()? != HOST_CONTROL_PREFIX {
        return None;
    }
    Some((parts.next()?, parts.next().unwrap_or("1")))
}

struct BridgeIpcEvent {
    name: String,
    payload: serde_json::Value,
}

impl std::fmt::Display for BridgeIpcEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = serde_json::to_string(&self.name).unwrap_or_else(|_| "\"event\"".to_string());
        let payload = serde_json::to_string(&self.payload).unwrap_or_else(|_| "null".to_string());
        write!(formatter, "SABINE_BRIDGE_EVENT\t{name}\t{payload}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn emitter_with_reader() -> (Child, BridgeEventEmitter) {
        let mut command = if cfg!(target_os = "windows") {
            let mut command = Command::new("cmd");
            command.args(["/d", "/c", "more"]);
            command
        } else {
            Command::new("cat")
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .expect("stdin reader");
        let writer = BridgeWriter::spawn(child.stdin.take().expect("stdin"));
        let emitter = BridgeEventEmitter {
            targets: Arc::new(Mutex::new(vec![BridgeTarget {
                window_id: child.id(),
                writer,
            }])),
        };
        (child, emitter)
    }

    #[test]
    fn emitter_detach_stops_broadcast() {
        let (mut child, emitter) = emitter_with_reader();
        assert!(emitter.emit("ping", serde_json::json!({})));
        emitter.detach(child.id());
        assert!(!emitter.emit("ping", serde_json::json!({})));
        let _ = child.kill();
        let _ = child.wait();
    }
}
