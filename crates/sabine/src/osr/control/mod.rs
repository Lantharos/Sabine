mod queue;

use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use super::transport::IpcStream;
use queue::{Batch, ControlQueue};

pub(crate) struct ControlWriter {
    queue: Arc<ControlQueue>,
    stream: IpcStream,
}

/// Carries the app's bridge messages to the current browser connection from
/// the thread that reads them, waiting while the browser catches up.
#[derive(Clone, Default)]
pub(crate) struct ControlRelay {
    writer: Arc<Mutex<Option<Arc<ControlWriter>>>>,
    page_frozen: Arc<AtomicBool>,
}

impl ControlRelay {
    pub(crate) fn connect(&self, writer: Option<Arc<ControlWriter>>) {
        if let Ok(mut current) = self.writer.lock() {
            *current = writer;
        }
    }

    pub(crate) fn set_page_frozen(&self, frozen: bool) {
        self.page_frozen.store(frozen, Ordering::Release);
    }

    /// Forwards a message to the page, and returns whether the page is
    /// frozen and has to thaw to take it.
    pub(crate) fn forward(&self, frame: Vec<u8>) -> bool {
        let writer = self.writer.lock().ok().and_then(|current| current.clone());
        if let Some(writer) = writer {
            let _ = writer.finish_send(writer.queue.push_relayed(frame));
        }
        self.page_frozen.swap(false, Ordering::AcqRel)
    }
}

/// A wheel turn the page has yet to receive, which later turns with the same
/// modifiers add to while it waits.
#[derive(Clone, Copy)]
pub(crate) struct Wheel {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) dx: i32,
    pub(crate) dy: i32,
    pub(crate) modifiers: u32,
}

impl Wheel {
    fn line(&self) -> String {
        let Self {
            x,
            y,
            dx,
            dy,
            modifiers,
        } = self;
        format!("mouse_wheel\t{x:.2}\t{y:.2}\t{dx}\t{dy}\t{modifiers}\n")
    }

    fn add(&mut self, newer: Self) -> bool {
        if self.modifiers != newer.modifiers {
            return false;
        }
        *self = Self {
            dx: self.dx.saturating_add(newer.dx),
            dy: self.dy.saturating_add(newer.dy),
            ..newer
        };
        true
    }
}

/// Gives the browser back the slot of an accelerated frame once the window
/// is done with the frame.
pub(crate) struct SlotRelease {
    writer: Arc<ControlWriter>,
    token: u64,
}

impl SlotRelease {
    pub(crate) fn new(writer: Arc<ControlWriter>, token: u64) -> Self {
        Self { writer, token }
    }
}

impl Drop for SlotRelease {
    fn drop(&mut self) {
        self.writer.release_accel(self.token);
    }
}

impl ControlWriter {
    pub(super) fn start(mut stream: IpcStream) -> io::Result<Self> {
        stream.set_write_timeout(Some(std::time::Duration::from_secs(5)))?;
        let owned = stream.try_clone()?;
        let queue = Arc::new(ControlQueue::new());
        let worker_queue = Arc::clone(&queue);
        thread::spawn(move || {
            let mut batch = Batch::default();
            while worker_queue.take(&mut batch) {
                if let Err(error) = batch.write_to(&mut stream) {
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

    /// Sends a message carrying bytes, waiting while the browser is behind,
    /// so it never runs on the window's thread.
    pub(super) fn send_bytes(&self, frame: Vec<u8>) -> Result<(), String> {
        self.finish_send(self.queue.push_relayed(frame))
    }

    pub(super) fn send_motion(&self, line: String) -> Result<(), String> {
        self.finish_send(self.queue.push_motion(line))
    }

    pub(super) fn send_wheel(&self, wheel: Wheel) -> Result<(), String> {
        self.finish_send(self.queue.push_wheel(wheel))
    }

    /// Sends where a finger moved, replacing where it had moved to if the
    /// browser has yet to hear that.
    pub(super) fn send_touch_move(&self, id: i32, line: String) -> Result<(), String> {
        self.finish_send(self.queue.push_touch_move(id, line))
    }

    /// Sends where the pen moved, keeping every sample a stroke is drawn
    /// from.
    pub(super) fn send_pen_move(&self, line: String) -> Result<(), String> {
        self.finish_send(self.queue.push_pen_move(line))
    }

    /// Gives the browser back the slot of an accelerated frame.
    pub(crate) fn release_accel(&self, token: u64) {
        self.queue
            .push_release(format_args!("accel_release\t{token}\n"));
    }

    /// Gives the browser back a shared memory slot it painted into.
    pub(crate) fn release_paint(&self, slot: u32, generation: u32) {
        self.queue
            .push_release(format_args!("paint_release\t{slot}\t{generation}\n"));
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
        self.queue.close();
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}
