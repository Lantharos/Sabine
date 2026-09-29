use std::{
    collections::VecDeque,
    io::{self, Write},
    sync::{Arc, Condvar, Mutex},
    thread,
};

use super::transport::IpcStream;

const MAX_QUEUED_CONTROLS: usize = 256;
const MAX_QUEUED_BYTES: usize = 64 * 1024 * 1024;
const MAX_RELAYED_CONTROLS: usize = MAX_QUEUED_CONTROLS / 2;
const MAX_RELAYED_BYTES: usize = MAX_QUEUED_BYTES / 2;

pub(crate) struct ControlWriter {
    queue: Arc<ControlQueue>,
    stream: IpcStream,
}

struct ControlQueue {
    state: Mutex<ControlQueueState>,
    ready: Condvar,
    space: Condvar,
}

/// Carries the app's bridge messages to the current browser connection from
/// the thread that reads them, waiting while the browser catches up.
#[derive(Clone, Default)]
pub(crate) struct ControlRelay(Arc<Mutex<Option<Arc<ControlWriter>>>>);

impl ControlRelay {
    pub(crate) fn connect(&self, writer: Option<Arc<ControlWriter>>) {
        if let Ok(mut current) = self.0.lock() {
            *current = writer;
        }
    }

    pub(crate) fn forward(&self, frame: Vec<u8>) {
        let writer = self.0.lock().ok().and_then(|current| current.clone());
        if let Some(writer) = writer {
            let _ = writer.finish_send(writer.queue.push_relayed(frame));
        }
    }
}

struct ControlQueueState {
    messages: VecDeque<ControlMessage>,
    closed: bool,
    bytes: usize,
    error: Option<String>,
}

enum ControlMessage {
    Motion(String),
    Ordered {
        line: String,
        coalescing_key: Option<ControlCoalescingKey>,
    },
    Relayed(Vec<u8>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlCoalescingKey {
    Focus,
    Lifecycle,
    Resize,
}

impl ControlWriter {
    pub(super) fn start(mut stream: IpcStream) -> io::Result<Self> {
        stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
        let owned = stream.try_clone()?;
        let queue = Arc::new(ControlQueue::new());
        let worker_queue = Arc::clone(&queue);
        thread::spawn(move || {
            while let Some(message) = worker_queue.next() {
                if let Err(error) = stream.write_all(&message.into_bytes()) {
                    worker_queue.fail(error);
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    break;
                }
            }
        });
        Ok(Self {
            queue,
            stream: owned,
        })
    }

    pub(super) fn send(&self, line: String) -> Result<(), String> {
        self.finish_send(self.queue.push_ordered(line))
    }

    pub(super) fn send_motion(&self, line: String) -> Result<(), String> {
        self.finish_send(self.queue.push_motion(line))
    }

    fn finish_send(&self, result: Result<(), String>) -> Result<(), String> {
        if let Err(error) = &result {
            self.queue.fail(io::Error::other(error.clone()));
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
        }
        result
    }
}

impl Drop for ControlWriter {
    fn drop(&mut self) {
        if let Ok(mut state) = self.queue.state.lock() {
            state.closed = true;
        }
        self.queue.ready.notify_one();
        self.queue.space.notify_all();
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

impl ControlQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(ControlQueueState {
                messages: VecDeque::new(),
                closed: false,
                bytes: 0,
                error: None,
            }),
            ready: Condvar::new(),
            space: Condvar::new(),
        }
    }

    fn push_relayed(&self, frame: Vec<u8>) -> Result<(), String> {
        let mut state = self.lock_open_state()?;
        while state.messages.len() >= MAX_RELAYED_CONTROLS
            || (state.bytes > 0 && state.bytes + frame.len() > MAX_RELAYED_BYTES)
        {
            state = self
                .space
                .wait(state)
                .map_err(|_| "control queue lock was poisoned".to_string())?;
            if state.closed {
                return Err("control writer is closed".to_string());
            }
        }
        state.bytes += frame.len();
        state.messages.push_back(ControlMessage::Relayed(frame));
        drop(state);
        self.ready.notify_one();
        Ok(())
    }

    fn push_ordered(&self, line: String) -> Result<(), String> {
        if line.len() > MAX_QUEUED_BYTES {
            return Err("control message exceeds 64 MiB".to_string());
        }
        let coalescing_key = control_coalescing_key(&line);
        let mut state = self.lock_open_state()?;
        if let Some(key) = coalescing_key
            && let Some(index) = state
                .messages
                .iter()
                .enumerate()
                .rev()
                .take_while(|(_, message)| message.is_coalescible_state())
                .find_map(|(index, message)| message.has_coalescing_key(key).then_some(index))
            && let Some(removed) = state.messages.remove(index)
        {
            state.bytes -= removed.len();
        }
        while state.messages.len() >= MAX_QUEUED_CONTROLS
            || state.bytes + line.len() > MAX_QUEUED_BYTES
        {
            let Some(index) = state.messages.iter().position(ControlMessage::is_motion) else {
                return Err("control queue is full".to_string());
            };
            if let Some(removed) = state.messages.remove(index) {
                state.bytes -= removed.len();
            }
        }
        state.bytes += line.len();
        state.messages.push_back(ControlMessage::Ordered {
            line,
            coalescing_key,
        });
        drop(state);
        self.ready.notify_one();
        Ok(())
    }

    fn push_motion(&self, line: String) -> Result<(), String> {
        if line.len() > MAX_QUEUED_BYTES {
            return Err("control message exceeds 64 MiB".to_string());
        }
        let mut state = self.lock_open_state()?;
        if matches!(state.messages.back(), Some(ControlMessage::Motion(_)))
            && let Some(removed) = state.messages.pop_back()
        {
            state.bytes -= removed.len();
        }
        if (state.messages.len() >= MAX_QUEUED_CONTROLS
            || state.bytes + line.len() > MAX_QUEUED_BYTES)
            && let Some(index) = state.messages.iter().position(ControlMessage::is_motion)
            && let Some(removed) = state.messages.remove(index)
        {
            state.bytes -= removed.len();
        }
        if state.messages.len() < MAX_QUEUED_CONTROLS
            && state.bytes + line.len() <= MAX_QUEUED_BYTES
        {
            state.bytes += line.len();
            state.messages.push_back(ControlMessage::Motion(line));
        }
        drop(state);
        self.ready.notify_one();
        Ok(())
    }

    fn lock_open_state(&self) -> Result<std::sync::MutexGuard<'_, ControlQueueState>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "control queue lock was poisoned".to_string())?;
        if state.closed {
            return Err(state
                .error
                .clone()
                .unwrap_or_else(|| "control writer is closed".to_string()));
        }
        Ok(state)
    }

    fn next(&self) -> Option<ControlMessage> {
        let mut state = self.state.lock().ok()?;
        loop {
            if state.closed {
                return None;
            }
            if let Some(message) = state.messages.pop_front() {
                state.bytes -= message.len();
                self.space.notify_all();
                return Some(message);
            }
            state = self.ready.wait(state).ok()?;
        }
    }

    fn fail(&self, error: io::Error) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            state.error = Some(error.to_string());
            state.messages.clear();
            state.bytes = 0;
        }
        self.ready.notify_all();
        self.space.notify_all();
    }
}

impl ControlMessage {
    fn len(&self) -> usize {
        match self {
            Self::Motion(line) | Self::Ordered { line, .. } => line.len(),
            Self::Relayed(frame) => frame.len(),
        }
    }

    fn is_motion(&self) -> bool {
        matches!(self, Self::Motion(_))
    }

    fn is_coalescible_state(&self) -> bool {
        matches!(
            self,
            Self::Ordered {
                coalescing_key: Some(_),
                ..
            }
        )
    }

    fn has_coalescing_key(&self, key: ControlCoalescingKey) -> bool {
        matches!(
            self,
            Self::Ordered {
                coalescing_key: Some(pending),
                ..
            } if *pending == key
        )
    }

    fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Motion(line) | Self::Ordered { line, .. } => line.into_bytes(),
            Self::Relayed(frame) => frame,
        }
    }
}

fn control_coalescing_key(line: &str) -> Option<ControlCoalescingKey> {
    match line.split_once('\t').map_or(line, |(command, _)| command) {
        "focus" => Some(ControlCoalescingKey::Focus),
        "lifecycle" => Some(ControlCoalescingKey::Lifecycle),
        "resize" => Some(ControlCoalescingKey::Resize),
        _ => None,
    }
}
