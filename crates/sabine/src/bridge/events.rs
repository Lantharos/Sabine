use std::{
    io::BufReader,
    process::{Child, ChildStdout, Command, Stdio},
    thread::{self, JoinHandle},
};

use sabine_bridge::BridgeHandlers;
use sabine_platform::PlatformEvent;

use super::emitter::BridgeEventEmitter;
use super::frame::Frame;
use super::request_dispatch::{BridgeIpcRequest, BridgeRequestDispatcher};
use super::writer::BridgeWriter;
use crate::launch::browser::HOST_CONTROL_PREFIX;

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
    let window = child.id();
    let emitter = BridgeEventEmitter::new(window, writer.clone());
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
        read_requests(stdout, window, &dispatcher, || {
            let _ = ready_sender.try_send(());
        });
        detach_emitter.detach(window);
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
    let window = child.id();
    emitter.attach(window, writer.clone());
    let stdout = child.stdout.take()?;
    let activity_emitter = emitter.clone();
    let dispatcher =
        BridgeRequestDispatcher::new(bridge_runtime, activity, emitter.clone(), writer);
    Some(thread::spawn(move || {
        read_requests(stdout, window, &dispatcher, || {});
        activity_emitter.detach(window);
    }))
}

fn read_requests(
    stdout: ChildStdout,
    window: u32,
    dispatcher: &BridgeRequestDispatcher,
    mut ready: impl FnMut(),
) {
    let mut reader = BufReader::new(stdout);
    while let Ok(Some(frame)) = Frame::read(&mut reader) {
        if frame.line == "SABINE_OSR_READY" {
            ready();
            continue;
        }
        if let Some(request) = BridgeIpcRequest::parse(frame, window) {
            dispatcher.submit(request);
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
