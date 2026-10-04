mod cues;
mod inbox;
mod pipeline;
mod session;
mod streams;

use std::{sync::Arc, thread::JoinHandle};

pub(super) use inbox::Control;
use inbox::Inbox;
use session::Session;

use super::gst::{self, Handle};
use super::present::Presenter;
use crate::media::command::{Events, PlayerOptions};

/// A media pipeline and its presenter, running on their own thread.
pub(super) struct Worker {
    inbox: Arc<Inbox>,
    thread: JoinHandle<()>,
}

impl Worker {
    pub(super) fn spawn(options: PlayerOptions, wayland: Handle, events: Events) -> Self {
        let inbox = Arc::new(Inbox::default());
        let thread_inbox = Arc::clone(&inbox);
        let thread = std::thread::Builder::new()
            .name("sabine-media".into())
            .spawn(move || {
                let session = gst::gst().and_then(|gst| {
                    let presenter = Presenter::new(gst, wayland)?;
                    Session::new(gst, options, presenter, thread_inbox, Arc::clone(&events))
                });
                match session {
                    Ok(session) => session.run(),
                    Err(message) => {
                        events(serde_json::json!({ "type": "error", "message": message }))
                    }
                }
            })
            .expect("could not start a media thread");
        Self { inbox, thread }
    }

    pub(super) fn send(&self, control: Control) {
        self.inbox.post(|mail| mail.controls.push(control));
    }

    /// Asks the player to wind down, returning its thread to join.
    pub(super) fn stop(self) -> JoinHandle<()> {
        self.inbox.post(|mail| mail.stop = true);
        self.thread
    }
}
