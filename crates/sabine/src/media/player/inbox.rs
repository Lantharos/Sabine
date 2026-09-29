use std::{
    sync::{Condvar, Mutex},
    time::Instant,
};

use crate::media::geometry::Frame;
use crate::media::gst::{Gst, Handle};
use crate::media::wayland::Target;

pub(in crate::media) enum Command {
    Play,
    Pause,
    Seek { time: f64, fast: bool },
    Rate(f64),
    Volume(f64),
    Muted(bool),
    Loop(bool),
    Tracks(TrackRequest),
    Frame(Option<Frame>),
    Target(Option<Target>),
    Occluded(bool),
}

/// A track choice from the page: `None` keeps the current choice and
/// `Some(None)` turns the track kind off.
#[derive(Default)]
pub(in crate::media) struct TrackRequest {
    pub(in crate::media) audio: Option<Option<String>>,
    pub(in crate::media) subtitle: Option<Option<String>>,
}

/// A bus message the player thread still has to handle; it owns the message.
pub(super) struct Message {
    gst: &'static Gst,
    pub(super) handle: Handle,
}

impl Message {
    pub(super) fn new(gst: &'static Gst, handle: Handle) -> Self {
        Self { gst, handle }
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        unsafe { (self.gst.gst_mini_object_unref)(self.handle.0) };
    }
}

pub(super) struct Cue {
    pub(super) text: String,
    pub(super) start: f64,
    pub(super) end: f64,
}

#[derive(Default)]
pub(super) struct Mail {
    pub(super) commands: Vec<Command>,
    pub(super) messages: Vec<Message>,
    pub(super) cues: Vec<Cue>,
    pub(super) picture: bool,
    pub(super) stop: bool,
}

/// Everything that wakes a player thread: page commands, bus messages,
/// decoded pictures and subtitle cues.
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

    /// Drops pending bus messages and pictures of a pipeline that stopped.
    pub(super) fn drop_messages(&self) {
        let mut mail = self.mail.lock().unwrap_or_else(|error| error.into_inner());
        let messages = std::mem::take(&mut mail.messages);
        mail.cues.clear();
        mail.picture = false;
        drop(mail);
        drop(messages);
    }

    pub(super) fn wait(&self, deadline: Option<Instant>) -> Mail {
        let mut mail = self.mail.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if mail.stop
                || mail.picture
                || !mail.commands.is_empty()
                || !mail.messages.is_empty()
                || !mail.cues.is_empty()
            {
                return std::mem::take(&mut *mail);
            }
            match deadline {
                Some(deadline) => {
                    let Some(timeout) = deadline.checked_duration_since(Instant::now()) else {
                        return Mail::default();
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
    }
}
