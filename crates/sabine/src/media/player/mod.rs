mod cues;
mod inbox;
mod pipeline;
mod session;
mod tracks;

use std::{sync::Arc, thread::JoinHandle};

use serde_json::Value;

use inbox::Inbox;
pub(super) use inbox::{Command, TrackRequest};
use session::Session;

use super::gst::{self, Handle};
use super::present::Presenter;
use super::wayland::Target;

/// Reports an event about one player to its page.
pub(crate) type Events = Arc<dyn Fn(Value) + Send + Sync>;

pub(super) struct PlayerOptions {
    pub(super) uri: String,
    pub(super) autoplay: bool,
    pub(super) looping: bool,
    pub(super) volume: f64,
    pub(super) muted: bool,
    pub(super) rate: f64,
}

/// A media pipeline and its presenter, running on their own thread.
pub(super) struct Player {
    inbox: Arc<Inbox>,
    thread: JoinHandle<()>,
}

impl Player {
    pub(super) fn spawn(
        options: PlayerOptions,
        target: Option<Target>,
        wayland: Handle,
        events: Events,
    ) -> Self {
        let inbox = Arc::new(Inbox::default());
        let thread_inbox = Arc::clone(&inbox);
        let thread = std::thread::Builder::new()
            .name("sabine-media".into())
            .spawn(move || {
                let session = gst::gst().and_then(|gst| {
                    let mut presenter = Presenter::new(gst, wayland)?;
                    presenter.set_target(target);
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

    pub(super) fn send(&self, command: Command) {
        self.inbox.post(|mail| mail.commands.push(command));
    }

    /// Asks the player to wind down, returning its thread to join.
    pub(super) fn stop(self) -> JoinHandle<()> {
        self.inbox.post(|mail| mail.stop = true);
        self.thread
    }
}
