use std::{
    panic::AssertUnwindSafe,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

use futures_util::FutureExt;
use sabine_bridge::{BridgeCommand, BridgeError, BridgeFuture, BridgeOutcome, BridgeResult};

use super::emitter::BridgeEventEmitter;
use super::frame::Frame;
use super::writer::BridgeWriter;
use crate::desktop::IdleWatches;

/// Starts the line of a page's bridge call.
pub(super) const REQUEST_PREFIX: &str = "SABINE_BRIDGE_REQUEST\t";
/// Sent to a window's pages when updates from `bridge.send` were dropped
/// because the window's queue was full.
const DROPPED_EVENT: &str = "sabine.bridge.dropped";
/// Calls one window may have waiting for a worker.
const MAX_PENDING_REQUESTS: usize = 128;

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
        let parts = frame
            .line
            .strip_prefix(REQUEST_PREFIX)?
            .splitn(5, '\t')
            .collect::<Vec<_>>();
        let [browser_id, id, origin, name, params] = parts[..] else {
            return None;
        };
        let params = serde_json::from_str(params).ok()?;
        Some(Self {
            reply: ReplyTo {
                browser_id: browser_id.to_string(),
                id: id.to_string(),
            },
            command: BridgeCommand {
                origin: Some(origin.to_string()).filter(|origin| !origin.is_empty()),
                name: name.to_string(),
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

/// What a window's calls run against and where their answers go.
struct WindowCalls {
    runtime: sabine_bridge::BridgeRuntime,
    activity: sabine_bridge::ActivityRegistry,
    idle: IdleWatches,
    emitter: BridgeEventEmitter,
    writer: BridgeWriter,
    waiting: AtomicUsize,
}

impl WindowCalls {
    fn run(&self, BridgeIpcRequest { reply, command }: BridgeIpcRequest) {
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            if let Some(outcome) = self
                .idle
                .dispatch(&command, &reply.browser_id, &self.emitter)
            {
                return outcome;
            }
            match self
                .activity
                .dispatch_bridge_command(&command, &reply.browser_id)
            {
                Some((result, updates)) => {
                    for update in &updates {
                        self.emitter.emit_activity_update(update);
                    }
                    BridgeOutcome::Ready(result)
                }
                None => self.runtime.dispatch_from_authorized_document(command),
            }
        }))
        .unwrap_or_else(|_| BridgeOutcome::Ready(Err(handler_panicked())));
        match outcome {
            BridgeOutcome::Ready(result) => {
                if let Some(answer) = reply.answer(result) {
                    self.writer.answer(answer);
                }
            }
            BridgeOutcome::Pending(future) => {
                sabine_platform::async_runtime().spawn(answer(reply, future, self.writer.clone()));
            }
        }
    }
}

struct Job {
    window: Arc<WindowCalls>,
    request: BridgeIpcRequest,
}

/// The workers every window's synchronous handlers run on, one per core and
/// at least four.
fn workers() -> &'static crossbeam_channel::Sender<Job> {
    static WORKERS: LazyLock<crossbeam_channel::Sender<Job>> = LazyLock::new(|| {
        let (jobs, queue) = crossbeam_channel::unbounded::<Job>();
        for _ in 0..worker_count() {
            let queue = queue.clone();
            thread::spawn(move || {
                for Job { window, request } in queue {
                    window.waiting.fetch_sub(1, Ordering::Relaxed);
                    window.run(request);
                }
            });
        }
        jobs
    });
    &WORKERS
}

fn worker_count() -> usize {
    thread::available_parallelism().map_or(4, |cores| cores.get().max(4))
}

/// Runs a window's bridge handlers off its reader thread. Synchronous
/// handlers run on the workers every window shares; async handlers are
/// awaited on Sabine's async runtime, so a handler that waits does not hold
/// a worker.
pub(super) struct BridgeRequestDispatcher {
    window: Arc<WindowCalls>,
    /// Set while updates are being dropped, so pages hear once per burst.
    dropping: AtomicBool,
}

impl BridgeRequestDispatcher {
    pub(super) fn new(
        runtime: sabine_bridge::BridgeRuntime,
        activity: sabine_bridge::ActivityRegistry,
        idle: IdleWatches,
        emitter: BridgeEventEmitter,
        writer: BridgeWriter,
    ) -> Self {
        Self {
            window: Arc::new(WindowCalls {
                runtime,
                activity,
                idle,
                emitter,
                writer,
                waiting: AtomicUsize::new(0),
            }),
            dropping: AtomicBool::new(false),
        }
    }

    pub(super) fn submit(&self, request: BridgeIpcRequest) {
        let waiting = &self.window.waiting;
        if waiting.fetch_add(1, Ordering::Relaxed) < MAX_PENDING_REQUESTS {
            self.dropping.store(false, Ordering::Relaxed);
            let _ = workers().send(Job {
                window: Arc::clone(&self.window),
                request,
            });
            return;
        }
        waiting.fetch_sub(1, Ordering::Relaxed);
        let notice = if request.reply.wants_answer() {
            request.reply.answer(Err(BridgeError::new(
                "Bridge request capacity is exhausted; retry later",
            )))
        } else if !self.dropping.swap(true, Ordering::Relaxed) {
            Some(dropped_notice(&request.command.name))
        } else {
            None
        };
        if let Some(notice) = notice {
            self.window.writer.answer(notice);
        }
    }
}

async fn answer(reply: ReplyTo, future: BridgeFuture, writer: BridgeWriter) {
    let result = AssertUnwindSafe(future)
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err(handler_panicked()));
    if let Some(answer) = reply.answer(result) {
        writer.answer(answer);
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

    #[test]
    fn waiting_async_handlers_do_not_hold_workers() {
        let waiting = worker_count() + 2;
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
            IdleWatches::default(),
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
