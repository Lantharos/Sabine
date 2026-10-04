use std::{panic::AssertUnwindSafe, sync::OnceLock, thread};

use futures_util::FutureExt;
use sabine_bridge::{BridgeCommand, BridgeError, BridgeFuture, BridgeOutcome, BridgeResult};

use super::frame::Frame;

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
    fn complete(self, result: BridgeResult) -> BridgeIpcResponse {
        BridgeIpcResponse {
            reply: self,
            result,
        }
    }
}

struct BridgeIpcResponse {
    reply: ReplyTo,
    result: BridgeResult,
}

impl BridgeIpcResponse {
    fn into_frame(self) -> Vec<u8> {
        let ReplyTo { browser_id, id } = self.reply;
        let (status, payload, body) = match self.result {
            Ok(response) => ("ok", response.result, response.body),
            Err(error) => (
                "error",
                serde_json::json!({ "message": error.message }),
                None,
            ),
        };
        let line = format!("SABINE_BRIDGE_RESPONSE\t{browser_id}\t{id}\t{status}\t{payload}");
        Frame::encode(&line, body.as_deref())
    }
}

/// Runs bridge handlers off the window's reader thread. Synchronous handlers
/// run on a small pool of workers; async handlers are awaited on one shared
/// runtime, so a handler that waits does not hold a worker.
pub(super) struct BridgeRequestDispatcher {
    requests: crossbeam_channel::Sender<BridgeIpcRequest>,
    responses: crossbeam_channel::Sender<BridgeIpcResponse>,
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
            crossbeam_channel::bounded::<BridgeIpcResponse>(MAX_PENDING_RESPONSES);

        for _ in 0..REQUEST_WORKERS {
            let requests = request_receiver.clone();
            let responses = response_sender.clone();
            let runtime = runtime.clone();
            let activity = activity.clone();
            let emitter = activity_emitter.clone();
            thread::spawn(move || {
                while let Ok(BridgeIpcRequest { reply, command }) = requests.recv() {
                    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        match activity.dispatch_bridge_command(&command) {
                            Some((result, update)) => {
                                if let (Ok(_), Some(update)) = (result.as_ref(), update.as_ref()) {
                                    let _ = emitter.emit_activity_update(update);
                                }
                                BridgeOutcome::Ready(result)
                            }
                            None => runtime.dispatch_from_authorized_document(command),
                        }
                    }))
                    .unwrap_or_else(|_| BridgeOutcome::Ready(Err(handler_panicked())));
                    match outcome {
                        BridgeOutcome::Ready(result) => {
                            if responses.send(reply.complete(result)).is_err() {
                                break;
                            }
                        }
                        BridgeOutcome::Pending(future) => {
                            async_handlers().spawn(answer(reply, future, responses.clone()));
                        }
                    }
                }
            });
        }

        thread::spawn(move || {
            while let Ok(response) = response_receiver.recv() {
                if !writer.send(response.into_frame().into()) {
                    break;
                }
            }
        });

        Self {
            requests: request_sender,
            responses: response_sender,
        }
    }

    pub(super) fn submit(&self, request: BridgeIpcRequest) {
        let (request, message) = match self.requests.try_send(request) {
            Ok(()) => return,
            Err(crossbeam_channel::TrySendError::Full(request)) => {
                (request, "Bridge request capacity is exhausted; retry later")
            }
            Err(crossbeam_channel::TrySendError::Disconnected(request)) => {
                (request, "Bridge request executor is unavailable")
            }
        };
        let _ = self
            .responses
            .try_send(request.reply.complete(Err(BridgeError::new(message))));
    }
}

async fn answer(
    reply: ReplyTo,
    future: BridgeFuture,
    responses: crossbeam_channel::Sender<BridgeIpcResponse>,
) {
    let result = AssertUnwindSafe(future)
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err(handler_panicked()));
    let _ = responses.send(reply.complete(result));
}

fn handler_panicked() -> BridgeError {
    BridgeError::new("Bridge handler panicked")
}

/// The runtime that awaits async bridge handlers for every window, started
/// the first time one is called.
fn async_handlers() -> &'static tokio::runtime::Handle {
    static HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();
    HANDLE.get_or_init(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("the bridge's async runtime starts");
        let handle = runtime.handle().clone();
        thread::Builder::new()
            .name("sabine-bridge-async".into())
            .spawn(move || runtime.block_on(std::future::pending::<()>()))
            .expect("the bridge's async runtime thread starts");
        handle
    })
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::BufRead;
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::time::Duration;

    use sabine_bridge::{BridgeHandlers, BridgeRegistry, BridgeResponse, BridgeRuntime};

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
        let mut registry = BridgeRegistry::default();
        registry.register("wait");
        let mut window = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("cat");
        let writer = BridgeWriter::spawn(window.stdin.take().expect("stdin"));
        let dispatcher = BridgeRequestDispatcher::new(
            BridgeRuntime::new(handlers, registry),
            sabine_bridge::ActivityRegistry::default(),
            BridgeEventEmitter::new(window.id(), writer.clone()),
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
