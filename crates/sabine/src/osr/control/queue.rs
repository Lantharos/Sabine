use std::{
    collections::VecDeque,
    fmt::Write as _,
    io::{self, Write},
    sync::{Condvar, Mutex, MutexGuard},
};

use super::Wheel;

const MAX_QUEUED_CONTROLS: usize = 256;
const MAX_QUEUED_BYTES: usize = 64 * 1024 * 1024;
const MAX_RELAYED_CONTROLS: usize = MAX_QUEUED_CONTROLS / 2;
const MAX_RELAYED_BYTES: usize = MAX_QUEUED_BYTES / 2;
/// Messages at least this long go to the browser as they are, instead of
/// being copied into the batch the writer sends.
const UNBATCHED_BYTES: usize = 64 * 1024;

pub(super) struct ControlQueue {
    state: Mutex<ControlQueueState>,
    ready: Condvar,
    space: Condvar,
}

struct ControlQueueState {
    messages: VecDeque<ControlMessage>,
    /// The frame slots the window let go of, which the browser takes back
    /// whatever else waits. They never count against the queue's limits.
    releases: String,
    closed: bool,
    bytes: usize,
    error: Option<String>,
}

enum ControlMessage {
    Motion(String),
    Wheel {
        wheel: Wheel,
        line: String,
    },
    TouchMove {
        id: i32,
        line: String,
    },
    PenMove(String),
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

/// Everything the writer sends to the browser in one go.
#[derive(Default)]
pub(super) struct Batch {
    messages: VecDeque<ControlMessage>,
    releases: String,
    bytes: Vec<u8>,
}

impl Batch {
    pub(super) fn write_to(&mut self, stream: &mut impl Write) -> io::Result<()> {
        self.bytes.clear();
        self.bytes.extend_from_slice(self.releases.as_bytes());
        self.releases.clear();
        for message in self.messages.drain(..) {
            let bytes = message.bytes();
            if bytes.len() < UNBATCHED_BYTES {
                self.bytes.extend_from_slice(bytes);
                continue;
            }
            stream.write_all(&self.bytes)?;
            self.bytes.clear();
            stream.write_all(bytes)?;
        }
        stream.write_all(&self.bytes)
    }
}

impl ControlQueue {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(ControlQueueState {
                messages: VecDeque::new(),
                releases: String::new(),
                closed: false,
                bytes: 0,
                error: None,
            }),
            ready: Condvar::new(),
            space: Condvar::new(),
        }
    }

    pub(super) fn push_relayed(&self, frame: Vec<u8>) -> Result<(), String> {
        let mut state = self.lock_open_state()?;
        while state.messages.len() >= MAX_RELAYED_CONTROLS
            || (state.bytes > 0 && state.bytes + frame.len() > MAX_RELAYED_BYTES)
        {
            state = self.wait_for_space(state)?;
        }
        state.bytes += frame.len();
        state.messages.push_back(ControlMessage::Relayed(frame));
        drop(state);
        self.ready.notify_one();
        Ok(())
    }

    /// Queues a message the browser must receive, after making room by
    /// dropping input it can do without or by waiting for the writer.
    pub(super) fn push_ordered(&self, line: String) -> Result<(), String> {
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
            match state.messages.iter().position(ControlMessage::is_lossy) {
                Some(index) => {
                    if let Some(removed) = state.messages.remove(index) {
                        state.bytes -= removed.len();
                    }
                }
                None => state = self.wait_for_space(state)?,
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

    pub(super) fn push_release(&self, release: std::fmt::Arguments<'_>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.closed {
            return;
        }
        let _ = state.releases.write_fmt(release);
        drop(state);
        self.ready.notify_one();
    }

    pub(super) fn push_motion(&self, line: String) -> Result<(), String> {
        let mut state = self.lock_open_state()?;
        if matches!(state.messages.back(), Some(ControlMessage::Motion(_)))
            && let Some(removed) = state.messages.pop_back()
        {
            state.bytes -= removed.len();
        }
        self.push_lossy(state, ControlMessage::Motion(line));
        Ok(())
    }

    pub(super) fn push_wheel(&self, wheel: Wheel) -> Result<(), String> {
        let mut state = self.lock_open_state()?;
        let queue = &mut *state;
        if let Some(ControlMessage::Wheel {
            wheel: queued,
            line,
        }) = queue.messages.back_mut()
            && queued.add(wheel)
        {
            let merged = queued.line();
            queue.bytes = queue.bytes - line.len() + merged.len();
            *line = merged;
            return Ok(());
        }
        let line = wheel.line();
        self.push_lossy(state, ControlMessage::Wheel { wheel, line });
        Ok(())
    }

    pub(super) fn push_touch_move(&self, id: i32, line: String) -> Result<(), String> {
        let mut state = self.lock_open_state()?;
        let queue = &mut *state;
        let pending = queue
            .messages
            .iter_mut()
            .rev()
            .take_while(|message| matches!(message, ControlMessage::TouchMove { .. }))
            .find_map(|message| match message {
                ControlMessage::TouchMove { id: queued, line } if *queued == id => Some(line),
                _ => None,
            });
        if let Some(pending) = pending {
            let replaced = std::mem::replace(pending, line);
            queue.bytes = queue.bytes - replaced.len() + pending.len();
            return Ok(());
        }
        self.push_lossy(state, ControlMessage::TouchMove { id, line });
        Ok(())
    }

    pub(super) fn push_pen_move(&self, line: String) -> Result<(), String> {
        let state = self.lock_open_state()?;
        self.push_lossy(state, ControlMessage::PenMove(line));
        Ok(())
    }

    /// Queues input the browser can do without, giving way to whatever must
    /// reach it when the queue is full.
    fn push_lossy(&self, mut state: MutexGuard<'_, ControlQueueState>, message: ControlMessage) {
        let len = message.len();
        if (state.messages.len() >= MAX_QUEUED_CONTROLS || state.bytes + len > MAX_QUEUED_BYTES)
            && let Some(index) = state.messages.iter().position(ControlMessage::is_lossy)
            && let Some(removed) = state.messages.remove(index)
        {
            state.bytes -= removed.len();
        }
        if state.messages.len() < MAX_QUEUED_CONTROLS && state.bytes + len <= MAX_QUEUED_BYTES {
            state.bytes += len;
            state.messages.push_back(message);
        }
        drop(state);
        self.ready.notify_one();
    }

    fn lock_open_state(&self) -> Result<MutexGuard<'_, ControlQueueState>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "control queue lock was poisoned".to_string())?;
        state.open()?;
        Ok(state)
    }

    fn wait_for_space<'a>(
        &self,
        state: MutexGuard<'a, ControlQueueState>,
    ) -> Result<MutexGuard<'a, ControlQueueState>, String> {
        let state = self
            .space
            .wait(state)
            .map_err(|_| "control queue lock was poisoned".to_string())?;
        state.open()?;
        Ok(state)
    }

    /// Waits for what the browser has yet to receive and moves all of it
    /// into `batch`. Returns false once the queue closed.
    pub(super) fn take(&self, batch: &mut Batch) -> bool {
        let Ok(mut state) = self.state.lock() else {
            return false;
        };
        loop {
            if state.closed {
                return false;
            }
            if !state.messages.is_empty() || !state.releases.is_empty() {
                std::mem::swap(&mut state.messages, &mut batch.messages);
                std::mem::swap(&mut state.releases, &mut batch.releases);
                state.bytes = 0;
                drop(state);
                self.space.notify_all();
                return true;
            }
            let Ok(waited) = self.ready.wait(state) else {
                return false;
            };
            state = waited;
        }
    }

    pub(super) fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
        }
        self.ready.notify_one();
        self.space.notify_all();
    }

    pub(super) fn fail(&self, error: io::Error) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            state.error = Some(error.to_string());
            state.messages.clear();
            state.releases.clear();
            state.bytes = 0;
        }
        self.ready.notify_all();
        self.space.notify_all();
    }
}

impl ControlQueueState {
    fn open(&self) -> Result<(), String> {
        if !self.closed {
            return Ok(());
        }
        Err(self
            .error
            .clone()
            .unwrap_or_else(|| "control writer is closed".to_string()))
    }
}

impl ControlMessage {
    fn len(&self) -> usize {
        self.bytes().len()
    }

    fn bytes(&self) -> &[u8] {
        match self {
            Self::Motion(line)
            | Self::Wheel { line, .. }
            | Self::TouchMove { line, .. }
            | Self::PenMove(line)
            | Self::Ordered { line, .. } => line.as_bytes(),
            Self::Relayed(frame) => frame,
        }
    }

    fn is_lossy(&self) -> bool {
        matches!(
            self,
            Self::Motion(_) | Self::Wheel { .. } | Self::TouchMove { .. } | Self::PenMove(_)
        )
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
}

fn control_coalescing_key(line: &str) -> Option<ControlCoalescingKey> {
    match line.split_once('\t').map_or(line, |(command, _)| command) {
        "focus" => Some(ControlCoalescingKey::Focus),
        "lifecycle" => Some(ControlCoalescingKey::Lifecycle),
        "resize" => Some(ControlCoalescingKey::Resize),
        _ => None,
    }
}
