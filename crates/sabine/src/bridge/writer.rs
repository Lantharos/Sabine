use std::{io::Write, process::ChildStdin, sync::Arc, thread};

use crossbeam_channel::{Sender, select_biased};

/// Messages queued for one window before a sender waits for it to catch up.
const QUEUED_MESSAGES: usize = 512;

/// Writes bridge messages to one window's host, in order. Answers to the
/// page's calls go ahead of queued events and never wait, so the workers
/// every window shares are never held up by one that is behind.
#[derive(Clone)]
pub(crate) struct BridgeWriter {
    events: Sender<Arc<[u8]>>,
    answers: Sender<Arc<[u8]>>,
}

impl BridgeWriter {
    pub(super) fn spawn(mut stdin: ChildStdin) -> Self {
        let (events, queued_events) = crossbeam_channel::bounded::<Arc<[u8]>>(QUEUED_MESSAGES);
        let (answers, queued_answers) = crossbeam_channel::unbounded::<Arc<[u8]>>();
        thread::spawn(move || {
            loop {
                let frame = select_biased! {
                    recv(queued_answers) -> frame => frame,
                    recv(queued_events) -> frame => frame,
                };
                let Ok(frame) = frame else {
                    break;
                };
                if stdin.write_all(&frame).is_err() {
                    break;
                }
            }
        });
        Self { events, answers }
    }

    /// Queues a message, waiting while the window is behind. Returns false
    /// once the window has closed.
    pub(super) fn send(&self, frame: Arc<[u8]>) -> bool {
        self.events.send(frame).is_ok()
    }

    /// Queues an answer to one of the page's calls. Returns false once the
    /// window has closed.
    pub(super) fn answer(&self, frame: Arc<[u8]>) -> bool {
        self.answers.send(frame).is_ok()
    }
}
