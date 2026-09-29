use std::{io::Write, process::ChildStdin, sync::Arc, thread};

/// Messages queued for one window before a sender waits for it to catch up.
const QUEUED_MESSAGES: usize = 512;

/// Writes bridge messages to one window's host, in order.
#[derive(Clone)]
pub(crate) struct BridgeWriter {
    sender: crossbeam_channel::Sender<Arc<[u8]>>,
}

impl BridgeWriter {
    pub(super) fn spawn(mut stdin: ChildStdin) -> Self {
        let (sender, receiver) = crossbeam_channel::bounded::<Arc<[u8]>>(QUEUED_MESSAGES);
        thread::spawn(move || {
            while let Ok(frame) = receiver.recv() {
                if stdin.write_all(&frame).is_err() {
                    break;
                }
            }
        });
        Self { sender }
    }

    /// Queues a message, waiting while the window is behind. Returns false
    /// once the window has closed.
    pub(super) fn send(&self, frame: Arc<[u8]>) -> bool {
        self.sender.send(frame).is_ok()
    }
}
