use std::{
    io::BufReader,
    process::{Child, ChildStdout, Command, Stdio},
    thread::{self, JoinHandle},
};

use sabine_platform::{PlatformEvent, SingleInstancePolicy};

use super::emitter::BridgeEventEmitter;
use super::frame::Frame;
use super::request_dispatch::{BridgeIpcRequest, BridgeRequestDispatcher};
use super::writer::BridgeWriter;
use crate::launch::browser::HOST_CONTROL_PREFIX;
use crate::window::{CLOSE_REQUESTED_LINE, WindowListeners};

pub(crate) fn platform_event_payload(event: PlatformEvent) -> (&'static str, serde_json::Value) {
    match event {
        PlatformEvent::OpenUrls(_) => ("app.openUrlsAvailable", serde_json::Value::Null),
        PlatformEvent::Tray(activation) => (
            "tray.activate",
            serde_json::json!({
                "trayId": activation.tray_id,
                "itemId": activation.item_id,
                "action": activation.action,
                "checked": activation.checked,
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
        PlatformEvent::GlobalShortcutFailed(failure) => (
            "globalShortcut.failed",
            serde_json::json!({
                "id": failure.id,
                "action": failure.action,
                "message": failure.message,
            }),
        ),
        PlatformEvent::SingleInstance(activation) => (
            "singleInstance.activate",
            serde_json::json!({
                "policy": single_instance_policy_name(activation.policy),
                "arguments": activation.arguments,
                "workingDirectory": activation.working_directory,
                "activationToken": activation.activation_token,
            }),
        ),
    }
}

fn single_instance_policy_name(policy: SingleInstancePolicy) -> &'static str {
    match policy {
        SingleInstancePolicy::AllowMultiple => "allowMultiple",
        SingleInstancePolicy::ReuseExisting => "reuseExisting",
        SingleInstancePolicy::FocusExisting => "focusExisting",
    }
}

pub(crate) fn prepare_bridge_command(command: &mut Command) {
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
}

pub(crate) struct BridgeDispatch {
    pub(crate) thread: JoinHandle<()>,
    pub(crate) ready: crossbeam_channel::Receiver<()>,
}

/// Attaches an OSR-host child to the app's bridge emitter and dispatches the
/// window's bridge requests through the app's handlers.
pub(crate) fn spawn_bridge_dispatch(
    child: &mut Child,
    bridge_runtime: sabine_bridge::BridgeRuntime,
    activity: sabine_bridge::ActivityRegistry,
    emitter: &BridgeEventEmitter,
    listeners: WindowListeners,
) -> BridgeDispatch {
    let writer = BridgeWriter::spawn(
        child
            .stdin
            .take()
            .expect("the window host's input is piped"),
    );
    let window = child.id();
    emitter.attach(window, writer.clone(), &activity);
    let stdout = child
        .stdout
        .take()
        .expect("the window host's output is piped");
    let (ready_sender, ready) = crossbeam_channel::bounded(1);
    let emitter = emitter.clone();
    let dispatcher =
        BridgeRequestDispatcher::new(bridge_runtime, activity.clone(), emitter.clone(), writer);
    let thread = thread::spawn(move || {
        let host = WindowHost {
            window,
            dispatcher: &dispatcher,
            listeners: &listeners,
            emitter: &emitter,
        };
        host.read_requests(stdout, || {
            let _ = ready_sender.try_send(());
        });
        activity.release_window(window);
        emitter.detach(window);
        listeners.closed(window);
    });
    BridgeDispatch { thread, ready }
}

/// What a window host's lines reach in the app.
struct WindowHost<'a> {
    window: u32,
    dispatcher: &'a BridgeRequestDispatcher,
    listeners: &'a WindowListeners,
    emitter: &'a BridgeEventEmitter,
}

impl WindowHost<'_> {
    fn read_requests(&self, stdout: ChildStdout, mut ready: impl FnMut()) {
        let mut reader = BufReader::new(stdout);
        while let Ok(Some(frame)) = Frame::read(&mut reader) {
            if frame.line == "SABINE_OSR_READY" {
                ready();
                continue;
            }
            if frame.line == CLOSE_REQUESTED_LINE {
                if self.listeners.allows_close(self.window) {
                    self.emitter.close_window(self.window);
                }
                continue;
            }
            if self.listeners.deliver(self.window, &frame.line) {
                continue;
            }
            if let Some(request) = BridgeIpcRequest::parse(frame, self.window) {
                self.dispatcher.submit(request);
            }
        }
    }
}

pub(crate) fn parse_host_control(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.splitn(3, '\t');
    if parts.next()? != HOST_CONTROL_PREFIX {
        return None;
    }
    Some((parts.next()?, parts.next().unwrap_or("1")))
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::BufRead;

    use sabine_bridge::{ActivityRegistry, BridgeHandlers, BridgeRuntime};

    use super::*;
    use crate::window::CloseRequestListener;

    fn lines_after_close_request(allowed: bool) -> Vec<String> {
        let mut window = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("cat");
        let mut host = Command::new("echo")
            .arg(CLOSE_REQUESTED_LINE)
            .stdout(Stdio::piped())
            .spawn()
            .expect("echo");
        let writer = BridgeWriter::spawn(window.stdin.take().expect("stdin"));
        let emitter = BridgeEventEmitter::default();
        emitter.attach(window.id(), writer.clone(), &ActivityRegistry::default());
        let dispatcher = BridgeRequestDispatcher::new(
            BridgeRuntime::new(BridgeHandlers::default()),
            ActivityRegistry::default(),
            emitter.clone(),
            writer,
        );
        let listeners = WindowListeners {
            close_request: Some(CloseRequestListener::new(move |_| allowed)),
            ..WindowListeners::default()
        };
        WindowHost {
            window: window.id(),
            dispatcher: &dispatcher,
            listeners: &listeners,
            emitter: &emitter,
        }
        .read_requests(host.stdout.take().expect("stdout"), || {});
        assert!(emitter.emit_to(window.id(), "end", serde_json::Value::Null));
        let mut lines = Vec::new();
        for line in BufReader::new(window.stdout.take().expect("stdout")).lines() {
            let line = line.expect("line");
            let last = line.starts_with("SABINE_BRIDGE_EVENT");
            lines.push(line);
            if last {
                break;
            }
        }
        let _ = window.kill();
        let _ = window.wait();
        let _ = host.wait();
        lines
    }

    #[test]
    fn a_window_asked_to_close_closes_only_when_the_app_agrees() {
        let close = format!("{HOST_CONTROL_PREFIX}\tclose\t1");
        assert_eq!(lines_after_close_request(true).first(), Some(&close));
        assert_eq!(lines_after_close_request(false).len(), 1);
    }
}
