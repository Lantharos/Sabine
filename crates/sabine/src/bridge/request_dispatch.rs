use std::{
    panic::AssertUnwindSafe,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use futures_util::FutureExt;
use sabine_bridge::{BridgeCommand, BridgeError, BridgeFuture, BridgeOutcome, BridgeResult};

use super::frame::Frame;

/// Sent to a window's pages when updates from `bridge.send` were dropped
/// because the window's queue was full.
const DROPPED_EVENT: &str = "sabine.bridge.dropped";
const MAX_PENDING_REQUESTS: usize = 128;
const MAX_PENDING_RESPONSES: usize = MAX_PENDING_REQUESTS + REQUEST_WORKERS;
const REQUEST_WORKERS: usize = 4;

pub(super) struct BridgeIpcRequest {
    reply: ReplyTo,
    command: BridgeCommand,
}

struct ReplyTo {
    browser_id: String,
    id: String,
}

impl BridgeIpcRequest {
    pub(super) fn parse(frame: Frame, window: u32) -> Option<Self> {
        let parts = frame.line.splitn(6, '\t').collect::<Vec<_>>();
        if parts.first().copied()? != "SABINE_BRIDGE_REQUEST" || parts.len() != 6 {
            return None;
        }
        let params = serde_json::from_str(parts[5]).ok()?;
        Some(Self {
            reply: ReplyTo {
                browser_id: parts[1].to_string(),
                id: parts[2].to_string(),
            },
            command: BridgeCommand {
                origin: Some(parts[3].to_string()).filter(|origin| !origin.is_empty()),
                name: parts[4].to_string(),
                params,
                body: frame.body,
                window: Some(window),
            },
        })
    }
}

impl ReplyTo {
    /// Updates the page sent with `bridge.send` carry no id and want no answer.
    fn wants_answer(&self) -> bool {
        !self.id.is_empty()
    }

    fn answer(self, result: BridgeResult) -> Option<Arc<[u8]>> {
        if !self.wants_answer() {
            return None;
        }
        let Self { browser_id, id } = self;
        let (status, payload, body) = match result {
            Ok(response) => ("ok", response.result, response.body),
            Err(error) => (
                "error",
                serde_json::json!({ "message": error.message }),
                None,
            ),
        };
        let line = format!("SABINE_BRIDGE_RESPONSE\t{browser_id}\t{id}\t{status}\t{payload}");
        Some(Frame::shared(&line, body.as_deref()))
    }
}

/// Tells the window's pages that updates sent to `command` were dropped.
fn dropped_notice(command: &str) -> Arc<[u8]> {
    let line =
        super::emitter::event_line(DROPPED_EVENT, &serde_json::json!({ "command": command }));
    Frame::shared(&line, None)
}

/// Runs bridge handlers off the window's reader thread. Synchronous handlers
/// run on a small pool of workers; async handlers are awaited on Sabine's
/// async runtime, so a handler that waits does not hold a worker.
pub(super) struct BridgeRequestDispatcher {
    requests: crossbeam_channel::Sender<BridgeIpcRequest>,
    responses: crossbeam_channel::Sender<Arc<[u8]>>,
    /// Set while updates are being dropped, so pages hear once per burst.
    dropping: AtomicBool,
}

impl BridgeRequestDispatcher {
    pub(super) fn new(
        runtime: sabine_bridge::BridgeRuntime,
        activity: sabine_bridge::ActivityRegistry,
        activity_emitter: super::emitter::BridgeEventEmitter,
        writer: super::writer::BridgeWriter,
    ) -> Self {
        let (request_sender, request_receiver) =
            crossbeam_channel::bounded::<BridgeIpcRequest>(MAX_PENDING_REQUESTS);
        let (response_sender, response_receiver) =
            crossbeam_channel::bounded::<Arc<[u8]>>(MAX_PENDING_RESPONSES);

        for _ in 0..REQUEST_WORKERS {
            let requests = request_receiver.clone();
            let responses = response_sender.clone();
            let runtime = runtime.clone();
            let activity = activity.clone();
            let emitter = activity_emitter.clone();
            thread::spawn(move || {
                while let Ok(BridgeIpcRequest { reply, command }) = requests.recv() {
                    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        match activity.dispatch_bridge_command(&command, &reply.browser_id) {
                            Some((result, updates)) => {
                                for update in &updates {
                                    emitter.emit_activity_update(update);
                                }
                                BridgeOutcome::Ready(result)
                            }
                            None => runtime.dispatch_from_authorized_document(command),
                        }
                    }))
                    .unwrap_or_else(|_| BridgeOutcome::Ready(Err(handler_panicked())));
                    match outcome {
                        BridgeOutcome::Ready(result) => {
                            if let Some(answer) = reply.answer(result)
                                && responses.send(answer).is_err()
                            {
                                break;
                            }
                        }
                        BridgeOutcome::Pending(future) => {
                            sabine_platform::async_runtime().spawn(answer(
                                reply,
                                future,
                                responses.clone(),
                            ));
                        }
                    }
                }
            });
        }

        thread::spawn(move || {
            while let Ok(frame) = response_receiver.recv() {
                if !writer.send(frame) {
                    break;
                }
            }
        });

        Self {
            requests: request_sender,
            responses: response_sender,
            dropping: AtomicBool::new(false),
        }
    }

    pub(super) fn submit(&self, request: BridgeIpcRequest) {
        let (request, message) = match self.requests.try_send(request) {
            Ok(()) => {
                self.dropping.store(false, Ordering::Relaxed);
                return;
            }
            Err(crossbeam_channel::TrySendError::Full(request)) => {
                (request, "Bridge request capacity is exhausted; retry later")
            }
            Err(crossbeam_channel::TrySendError::Disconnected(request)) => {
                (request, "Bridge request executor is unavailable")
            }
        };
        let notice = if request.reply.wants_answer() {
            request.reply.answer(Err(BridgeError::new(message)))
        } else if !self.dropping.swap(true, Ordering::Relaxed) {
            Some(dropped_notice(&request.command.name))
        } else {
            None
        };
        if let Some(notice) = notice {
            let _ = self.responses.try_send(notice);
        }
    }
}

async fn answer(
    reply: ReplyTo,
    future: BridgeFuture,
    responses: crossbeam_channel::Sender<Arc<[u8]>>,
) {
    let result = AssertUnwindSafe(future)
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err(handler_panicked()));
    if let Some(answer) = reply.answer(result) {
        let _ = responses.send(answer);
    }
}

fn handler_panicked() -> BridgeError {
    BridgeError::new("Bridge handler panicked")
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::BufRead;
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::time::Duration;

    use sabine_bridge::{BridgeHandlers, BridgeResponse, BridgeRuntime};

    use super::*;
    use crate::bridge::emitter::BridgeEventEmitter;
    use crate::bridge::writer::BridgeWriter;

    #[test]
    fn waiting_async_handlers_do_not_hold_workers() {
        let waiting = REQUEST_WORKERS + 2;
        let barrier = Arc::new(tokio::sync::Barrier::new(waiting));
        let mut handlers = BridgeHandlers::default();
        handlers.register_async("wait", move |_| {
            let barrier = Arc::clone(&barrier);
            async move {
                barrier.wait().await;
                Ok(BridgeResponse::json(serde_json::Value::Null))
            }
        });
        let mut window = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("cat");
        let writer = BridgeWriter::spawn(window.stdin.take().expect("stdin"));
        let dispatcher = BridgeRequestDispatcher::new(
            BridgeRuntime::new(handlers),
            sabine_bridge::ActivityRegistry::default(),
            BridgeEventEmitter::default(),
            writer,
        );
        for id in 0..waiting {
            let line = format!("SABINE_BRIDGE_REQUEST\t1\t{id}\t\twait\t{{}}");
            dispatcher.submit(BridgeIpcRequest::parse(Frame::line(line), 1).expect("request"));
        }

        let stdout = window.stdout.take().expect("stdout");
        let (lines, answered) = crossbeam_channel::unbounded();
        thread::spawn(move || {
            for line in std::io::BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
            {
                let _ = lines.send(line);
            }
        });
        for _ in 0..waiting {
            let line = answered
                .recv_timeout(Duration::from_secs(5))
                .expect("every waiting handler answers");
            assert!(line.contains("\tok\t"));
        }
        let _ = window.kill();
        let _ = window.wait();
    }
}
