use std::{
    io::BufReader,
    process::{Child, ChildStdout, Command, Stdio},
    sync::Arc,
    thread::{self, JoinHandle},
};

use sabine_platform::{PlatformEvent, PowerEvent, PowerSource, SingleInstancePolicy};

use super::emitter::BridgeEventEmitter;
use super::frame::Frame;
use super::listeners::WindowReports;
use super::request_dispatch::{BridgeIpcRequest, BridgeRequestDispatcher, REQUEST_PREFIX};
use super::writer::BridgeWriter;
use super::{APP_MENU_LINE, DIALOG_ANSWER_LINE};
use crate::desktop::IdleWatches;
use crate::host::SabineProcessHandle;
use crate::launch::browser::HOST_CONTROL_PREFIX;
use crate::window::{DISPLAYS_LINE, PAGE_LOST_LINE, PAGE_STARTED_LINE, WindowListeners};

/// A window host writes this when people move focus to its window.
pub(crate) const FOCUSED_LINE: &str = "SABINE_WINDOW_FOCUSED";
/// A window host writes this when someone chose to quit the app from its
/// menus.
pub(crate) const QUIT_REQUESTED_LINE: &str = "SABINE_QUIT_REQUESTED";

/// The event every page of the app hears, if any: a window's pages hear
/// about its notifications from the window itself.
pub(crate) fn platform_event_payload(
    event: PlatformEvent,
) -> Option<(&'static str, serde_json::Value)> {
    Some(match event {
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
                "arguments": activation.arguments.iter().map(|argument| argument.to_string_lossy()).collect::<Vec<_>>(),
                "workingDirectory": activation.working_directory.as_deref().map(std::path::Path::to_string_lossy),
                "activationToken": activation.activation_token,
            }),
        ),
        PlatformEvent::Notification { .. } => return None,
        PlatformEvent::Power(event) => power_event_payload(event),
    })
}

fn power_event_payload(event: PowerEvent) -> (&'static str, serde_json::Value) {
    match event {
        PowerEvent::Suspending => ("system.suspend", serde_json::Value::Null),
        PowerEvent::Resumed => ("system.resume", serde_json::Value::Null),
        PowerEvent::Locked => ("system.lock", serde_json::Value::Null),
        PowerEvent::Unlocked => ("system.unlock", serde_json::Value::Null),
        PowerEvent::Source(source) => ("system.powerSource", power_source_json(source)),
    }
}

pub(crate) fn power_source_json(source: PowerSource) -> serde_json::Value {
    serde_json::json!({
        "onBattery": source.on_battery,
        "batteryLevel": source.battery_level,
    })
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
    idle: IdleWatches,
    emitter: &BridgeEventEmitter,
    listeners: WindowListeners,
    process: SabineProcessHandle,
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
    let dispatcher = BridgeRequestDispatcher::new(
        bridge_runtime,
        activity.clone(),
        idle.clone(),
        emitter.clone(),
        writer,
    );
    let listeners = Arc::new(listeners);
    let reports = WindowReports::new(window, Arc::clone(&listeners), emitter.clone());
    let thread = thread::spawn(move || {
        let host = WindowHost {
            window,
            dispatcher: &dispatcher,
            listeners: &listeners,
            reports: &reports,
            emitter: &emitter,
            quit: &|| {
                process.quit();
            },
        };
        host.read_requests(stdout, || {
            let _ = ready_sender.try_send(());
        });
        activity.release_window(window);
        idle.release_window(window);
        emitter.detach(window);
        reports.closed();
    });
    BridgeDispatch { thread, ready }
}

/// What a window host's lines reach in the app. The page's calls and the
/// document changes they depend on are taken in order here; everything the
/// app's listeners hear goes to them.
struct WindowHost<'a> {
    window: u32,
    dispatcher: &'a BridgeRequestDispatcher,
    listeners: &'a WindowListeners,
    reports: &'a WindowReports,
    emitter: &'a BridgeEventEmitter,
    /// Ends the app, as someone chose to from the window's menus.
    quit: &'a dyn Fn(),
}

impl WindowHost<'_> {
    fn read_requests(&self, stdout: ChildStdout, mut ready: impl FnMut()) {
        let mut reader = BufReader::new(stdout);
        while let Ok(Some(frame)) = Frame::read(&mut reader) {
            let line = frame.line.as_str();
            if line == "SABINE_OSR_READY" {
                ready();
            } else if line == FOCUSED_LINE {
                self.emitter.focused(self.window);
            } else if line == PAGE_STARTED_LINE || line == PAGE_LOST_LINE {
                self.listeners.deliver(self.window, line);
            } else if let Some(displays) = line
                .strip_prefix(DISPLAYS_LINE)
                .and_then(|line| line.strip_prefix('\t'))
            {
                self.displays_reported(displays);
            } else if line == QUIT_REQUESTED_LINE {
                (self.quit)();
            } else if line.starts_with(DIALOG_ANSWER_LINE) {
                self.emitter.answer_dialog(line);
            } else if let Some(items) = line.strip_prefix(APP_MENU_LINE) {
                self.emitter.share_app_menu(self.window, items.trim_start());
            } else if line.starts_with(REQUEST_PREFIX) {
                if let Some(request) = BridgeIpcRequest::parse(frame, self.window) {
                    self.dispatcher.submit(request);
                }
            } else {
                self.reports.report(frame.line);
            }
        }
    }

    fn displays_reported(&self, json: &str) {
        let Ok(displays) = serde_json::from_str::<Vec<sabine_platform::Display>>(json) else {
            return;
        };
        if self.emitter.update_displays(&displays) {
            self.reports.displays_changed(displays);
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
    use crate::bridge::listeners::run_in_order;
    use crate::window::{CLOSE_REQUESTED_LINE, CloseRequestListener};

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
            IdleWatches::default(),
            emitter.clone(),
            writer,
        );
        let listeners = Arc::new(WindowListeners {
            close_request: Some(CloseRequestListener::new(move |_| allowed)),
            ..WindowListeners::default()
        });
        let reports = WindowReports::new(window.id(), Arc::clone(&listeners), emitter.clone());
        WindowHost {
            window: window.id(),
            dispatcher: &dispatcher,
            listeners: &listeners,
            reports: &reports,
            emitter: &emitter,
            quit: &|| {},
        }
        .read_requests(host.stdout.take().expect("stdout"), || {});
        let (answered, answer) = crossbeam_channel::bounded(1);
        run_in_order(move || {
            let _ = answered.send(());
        });
        answer.recv().expect("the listener answered");
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
