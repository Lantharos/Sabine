use std::io::BufRead;
use std::process::{Child, Command, Stdio};

use sabine_bridge::ActivityRegistry;

use super::*;
use crate::bridge::writer::BridgeWriter;

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
fn calls_for_the_apps_window_reach_the_window_focused_last() {
    let (mut first, first_writer) = window();
    let (mut second, second_writer) = window();
    let emitter = BridgeEventEmitter::default();
    emitter.attach(first.id(), first_writer, &ActivityRegistry::default());
    emitter.attach(second.id(), second_writer, &ActivityRegistry::default());
    assert!(emitter.show());
    emitter.focused(second.id());
    assert!(emitter.hide());
    assert_eq!(
        next_line(&mut first),
        format!("{HOST_CONTROL_PREFIX}\tshow\t1\n")
    );
    assert_eq!(
        next_line(&mut second),
        format!("{HOST_CONTROL_PREFIX}\thide\t1\n")
    );
    for mut child in [first, second] {
        let _ = child.kill();
        let _ = child.wait();
    }
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
