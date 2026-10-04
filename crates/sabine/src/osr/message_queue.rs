use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Condvar, Mutex},
};

use super::protocol::{FrameBytes, OsrMessage, OsrPaintBatch, PageCursorMessage, PaintRect};

const MAX_QUEUED_MESSAGES: usize = 256;
const MAX_QUEUED_BYTES: usize = 256 * 1024 * 1024;
const MAX_MERGED_RECTS: usize = 256;
const MAX_MERGED_BYTES: usize = 64 * 1024 * 1024;
const MESSAGE_DISPATCH_BUDGET: usize = 32;

/// What queueing a message did: whether the window must be woken, and which
/// accelerated frame slot a newer frame for the same surface replaced.
pub(super) struct Queued {
    pub(super) wake: bool,
    pub(super) replaced_slot: Option<u64>,
}

impl Queued {
    fn woken(wake: bool) -> Self {
        Self {
            wake,
            replaced_slot: None,
        }
    }
}

pub(super) struct MessageQueue {
    state: Mutex<MessageQueueState>,
    space_available: Condvar,
}

#[derive(Default)]
struct MessageQueueState {
    messages: VecDeque<OsrMessage>,
    wake_queued: bool,
    retained_bytes: usize,
    closed: bool,
}

impl MessageQueue {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(MessageQueueState::default()),
            space_available: Condvar::new(),
        }
    }

    pub(super) fn push(&self, message: OsrMessage) -> Queued {
        let Ok(mut state) = self.state.lock() else {
            return Queued::woken(false);
        };
        if state.closed {
            return Queued::woken(false);
        }
        let available = MAX_QUEUED_BYTES.saturating_sub(state.retained_bytes);
        let message = match message {
            OsrMessage::PaintBatch(incoming) => {
                let matching = state
                    .messages
                    .iter_mut()
                    .rev()
                    .take_while(|message| is_frame(message))
                    .find_map(|message| match message {
                        OsrMessage::PaintBatch(queued) if queued.surface == incoming.surface => {
                            Some(queued)
                        }
                        _ => None,
                    });
                if let Some(queued) = matching {
                    let previous_bytes = batch_retained_bytes(queued);
                    match merge_paint_batch(queued, incoming, available + previous_bytes) {
                        None => {
                            let merged_bytes = batch_retained_bytes(queued);
                            state.retained_bytes =
                                state.retained_bytes - previous_bytes + merged_bytes;
                            return Queued::woken(queue_wake(&mut state));
                        }
                        Some(incoming) => OsrMessage::PaintBatch(incoming),
                    }
                } else {
                    OsrMessage::PaintBatch(incoming)
                }
            }
            #[cfg(any(windows, target_os = "macos"))]
            OsrMessage::AccelFrame(incoming) => {
                let queued = state
                    .messages
                    .iter_mut()
                    .rev()
                    .take_while(|message| is_frame(message))
                    .find_map(|message| match message {
                        OsrMessage::AccelFrame(queued) if queued.surface == incoming.surface => {
                            Some(queued)
                        }
                        _ => None,
                    });
                if let Some(queued) = queued {
                    let replaced = std::mem::replace(queued, incoming);
                    return Queued {
                        wake: queue_wake(&mut state),
                        replaced_slot: Some(replaced.slot_token),
                    };
                }
                OsrMessage::AccelFrame(incoming)
            }
            message => message,
        };
        let bytes = message_retained_bytes(&message);
        while state.messages.len() >= MAX_QUEUED_MESSAGES
            || (!state.messages.is_empty()
                && state.retained_bytes.saturating_add(bytes) > MAX_QUEUED_BYTES)
        {
            let Ok(next) = self.space_available.wait(state) else {
                return Queued::woken(false);
            };
            state = next;
            if state.closed {
                return Queued::woken(false);
            }
        }
        state.retained_bytes += bytes;
        state.messages.push_back(message);
        Queued::woken(queue_wake(&mut state))
    }

    pub(super) fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
            state.messages.clear();
            state.retained_bytes = 0;
            state.wake_queued = false;
        }
        self.space_available.notify_all();
    }

    pub(super) fn drain_budgeted(&self) -> (VecDeque<OsrMessage>, bool) {
        let Ok(mut state) = self.state.lock() else {
            return (VecDeque::new(), false);
        };
        let count = state.messages.len().min(MESSAGE_DISPATCH_BUDGET);
        let messages: VecDeque<_> = state.messages.drain(..count).collect();
        state.retained_bytes -= messages.iter().map(message_retained_bytes).sum::<usize>();
        let remaining = !state.messages.is_empty();
        state.wake_queued = remaining;
        drop(state);
        self.space_available.notify_all();
        (messages, remaining)
    }
}

fn is_frame(message: &OsrMessage) -> bool {
    match message {
        OsrMessage::PaintBatch(_) => true,
        #[cfg(any(windows, target_os = "macos"))]
        OsrMessage::AccelFrame(_) => true,
        _ => false,
    }
}

fn queue_wake(state: &mut MessageQueueState) -> bool {
    if state.wake_queued {
        false
    } else {
        state.wake_queued = true;
        true
    }
}

fn merge_paint_batch(
    queued: &mut OsrPaintBatch,
    incoming: OsrPaintBatch,
    available: usize,
) -> Option<OsrPaintBatch> {
    if queued.surface != incoming.surface
        || queued.width != incoming.width
        || queued.height != incoming.height
        || queued.rects.len().saturating_add(incoming.rects.len()) > MAX_MERGED_RECTS
        || batch_retained_bytes(queued).saturating_add(batch_retained_bytes(&incoming))
            > available.min(MAX_MERGED_BYTES)
    {
        return Some(incoming);
    }
    queued.x = incoming.x;
    queued.y = incoming.y;
    for rect in incoming.rects {
        queued
            .rects
            .retain(|queued_rect| !rect_covers(&rect, queued_rect));
        queued.rects.push(rect);
    }
    None
}

fn rect_covers(newer: &PaintRect, older: &PaintRect) -> bool {
    let newer_right = i64::from(newer.x) + i64::from(newer.width);
    let newer_bottom = i64::from(newer.y) + i64::from(newer.height);
    let older_right = i64::from(older.x) + i64::from(older.width);
    let older_bottom = i64::from(older.y) + i64::from(older.height);
    newer.x <= older.x
        && newer.y <= older.y
        && newer_right >= older_right
        && newer_bottom >= older_bottom
}

fn batch_retained_bytes(batch: &OsrPaintBatch) -> usize {
    let mut inline_sources = HashSet::new();
    batch
        .rects
        .iter()
        .map(|rect| match &rect.bytes {
            FrameBytes::Inline { source, .. } => {
                if inline_sources.insert(Arc::as_ptr(source)) {
                    source.len()
                } else {
                    0
                }
            }
            #[cfg(unix)]
            FrameBytes::Shared { range, .. } => range.len(),
        })
        .sum()
}

fn message_retained_bytes(message: &OsrMessage) -> usize {
    match message {
        OsrMessage::PaintBatch(batch) => batch_retained_bytes(batch),
        OsrMessage::FatalError(text)
        | OsrMessage::GuestHidden(text)
        | OsrMessage::TooltipChanged(text) => text.capacity(),
        OsrMessage::Cursor(PageCursorMessage::Custom(image)) => image.rgba.capacity(),
        OsrMessage::ContextMenu { items, .. } => {
            items.iter().map(|item| item.label.capacity()).sum()
        }
        OsrMessage::BridgeRequest(frame) => {
            frame.line.capacity() + frame.body.as_ref().map_or(0, Vec::capacity)
        }
        OsrMessage::FocusRequested(text) => text.as_ref().map_or(0, String::capacity),
        OsrMessage::GuestCaptureRequested {
            browser_id,
            request_id,
            guest_id,
        } => browser_id.capacity() + request_id.capacity() + guest_id.capacity(),
        OsrMessage::DraggableRegionsChanged { drag, exclusion } => {
            (drag.capacity() + exclusion.capacity())
                * std::mem::size_of::<sabine_platform::WindowRegionRect>()
        }
        OsrMessage::DragStarted { content, .. } => {
            content.text.capacity() + content.html.capacity() + content.url.capacity()
        }
        OsrMessage::ImeSurroundingChanged { text, .. } => text.capacity(),
        _ => 0,
    }
}
