// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The Media Engine calls back on Media Foundation's own threads. Callbacks
// only post plain data here; every engine call happens on the player's thread,
// which owns the engine in a multithreaded COM apartment.

use std::{
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::Instant,
};

use windows::Win32::Graphics::DirectComposition::IDCompositionVisual;

use super::session::Session;
use crate::media::command::{self, Command, Events, PlayerOptions};
use crate::media::geometry::Frame;
use crate::render::Composition;

/// The visual a player shows its video in, inside the window's tree.
pub(super) struct Surface {
    pub(super) composition: Arc<Composition>,
    pub(super) visual: IDCompositionVisual,
}

// SAFETY: DirectComposition is free-threaded.
unsafe impl Send for Surface {}

/// What the window thread asks of a player.
pub(super) enum Order {
    Page(Command),
    Frame(Option<Frame>),
    Surface(Option<Surface>),
}

/// What the Media Engine reports.
pub(super) enum Report {
    Engine {
        event: u32,
        code: usize,
        result: u32,
    },
    TextTracks,
    Cue(Cue),
    CueEnded(u32),
    CuesCleared,
}

pub(super) struct Cue {
    pub(super) id: u32,
    pub(super) text: String,
    pub(super) start: f64,
    pub(super) end: f64,
}

#[derive(Default)]
pub(super) struct Mail {
    pub(super) orders: Vec<Order>,
    pub(super) reports: Vec<Report>,
    pub(super) stop: bool,
}

/// Everything that wakes a player thread.
#[derive(Default)]
pub(super) struct Inbox {
    mail: Mutex<Mail>,
    ready: Condvar,
}

impl Inbox {
    pub(super) fn post(&self, deliver: impl FnOnce(&mut Mail)) {
        let mut mail = self.mail.lock().unwrap_or_else(|error| error.into_inner());
        deliver(&mut mail);
        self.ready.notify_one();
    }

    pub(super) fn wait(&self, deadline: Option<Instant>) -> Mail {
        let mut mail = self.mail.lock().unwrap_or_else(|error| error.into_inner());
        while !mail.stop && mail.orders.is_empty() && mail.reports.is_empty() {
            match deadline {
                Some(deadline) => {
                    let Some(timeout) = deadline.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    mail = self
                        .ready
                        .wait_timeout(mail, timeout)
                        .unwrap_or_else(|error| error.into_inner())
                        .0;
                }
                None => {
                    mail = self
                        .ready
                        .wait(mail)
                        .unwrap_or_else(|error| error.into_inner())
                }
            }
        }
        std::mem::take(&mut *mail)
    }
}

/// A Media Engine running on its own thread.
pub(super) struct Worker {
    inbox: Arc<Inbox>,
    thread: JoinHandle<()>,
}

impl Worker {
    pub(super) fn spawn(options: PlayerOptions, events: Events) -> Self {
        let inbox = Arc::new(Inbox::default());
        let thread_inbox = Arc::clone(&inbox);
        let thread = std::thread::Builder::new()
            .name("sabine-media".into())
            .spawn(
                move || match Session::new(options, Arc::clone(&events), thread_inbox) {
                    Ok(session) => session.run(),
                    Err(message) => {
                        command::emit(&events, "error", serde_json::json!({ "message": message }))
                    }
                },
            )
            .expect("could not start a media thread");
        Self { inbox, thread }
    }

    pub(super) fn send(&self, order: Order) {
        self.inbox.post(|mail| mail.orders.push(order));
    }

    /// Asks the player to shut its engine down; the thread then ends on its own.
    pub(super) fn stop(self) -> JoinHandle<()> {
        self.inbox.post(|mail| mail.stop = true);
        self.thread
    }
}
