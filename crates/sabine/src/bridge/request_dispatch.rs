use std::thread;

use sabine_bridge::{BridgeCommand, BridgeError, BridgeResult};

use super::frame::Frame;

const MAX_PENDING_REQUESTS: usize = 128;
const MAX_PENDING_RESPONSES: usize = MAX_PENDING_REQUESTS + REQUEST_WORKERS;
const REQUEST_WORKERS: usize = 4;

pub(super) struct BridgeIpcRequest {
    browser_id: String,
    id: String,
    command: BridgeCommand,
}

impl BridgeIpcRequest {
    pub(super) fn parse(frame: Frame, window: u32) -> Option<Self> {
        let parts = frame.line.splitn(6, '\t').collect::<Vec<_>>();
        if parts.first().copied()? != "SABINE_BRIDGE_REQUEST" || parts.len() != 6 {
            return None;
        }
        let params = serde_json::from_str(parts[5]).ok()?;
        Some(Self {
            browser_id: parts[1].to_string(),
            id: parts[2].to_string(),
            command: BridgeCommand {
                origin: Some(parts[3].to_string()).filter(|origin| !origin.is_empty()),
                name: parts[4].to_string(),
                params,
                body: frame.body,
                window: Some(window),
            },
        })
    }

    fn complete(self, result: BridgeResult) -> BridgeIpcResponse {
        BridgeIpcResponse::from_result(self.browser_id, self.id, result)
    }

    fn overloaded(self) -> BridgeIpcResponse {
        self.complete(Err(BridgeError::new(
            "Bridge request capacity is exhausted; retry later",
        )))
    }
}

struct BridgeIpcResponse {
    browser_id: String,
    id: String,
    ok: bool,
    payload: serde_json::Value,
    body: Option<Vec<u8>>,
}

impl BridgeIpcResponse {
    fn from_result(browser_id: String, id: String, result: BridgeResult) -> Self {
        match result {
            Ok(response) => Self {
                browser_id,
                id,
                ok: true,
                payload: response.result,
                body: response.body,
            },
            Err(error) => Self {
                browser_id,
                id,
                ok: false,
                payload: serde_json::json!({ "message": error.message }),
                body: None,
            },
        }
    }

    fn into_frame(self) -> Vec<u8> {
        let status = if self.ok { "ok" } else { "error" };
        let line = format!(
            "SABINE_BRIDGE_RESPONSE\t{}\t{}\t{status}\t{}",
            self.browser_id, self.id, self.payload
        );
        Frame::encode(&line, self.body.as_deref())
    }
}

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
                while let Ok(request) = requests.recv() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if let Some((result, update)) =
                            activity.dispatch_bridge_command(&request.command)
                        {
                            if let (Ok(_), Some(update)) = (result.as_ref(), update.as_ref()) {
                                let _ = emitter.emit_activity_update(update);
                            }
                            result
                        } else {
                            runtime.dispatch_from_authorized_document(request.command.clone())
                        }
                    }))
                    .unwrap_or_else(|_| Err(BridgeError::new("Bridge handler panicked")));
                    if responses.send(request.complete(result)).is_err() {
                        break;
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
        match self.requests.try_send(request) {
            Ok(()) => {}
            Err(crossbeam_channel::TrySendError::Full(request)) => {
                let _ = self.responses.try_send(request.overloaded());
            }
            Err(crossbeam_channel::TrySendError::Disconnected(request)) => {
                let _ = self
                    .responses
                    .try_send(request.complete(Err(BridgeError::new(
                        "Bridge request executor is unavailable",
                    ))));
            }
        }
    }
}
