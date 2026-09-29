use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::inbox::{Command, Cue, Inbox, Message};
use super::pipeline::Pipeline;
use super::tracks::Tracks;
use super::{Events, PlayerOptions};
use crate::media::gst::{self, Gst, types};
use crate::media::present::Presenter;

const TIME_INTERVAL: Duration = Duration::from_millis(250);
/// A paused pipeline keeps its decoder, and NVIDIA's driver then wakes
/// hundreds of times a second; after this long it is torn down and rebuilt
/// at the same position when playback resumes.
const PARK_DELAY: Duration = Duration::from_secs(5);

/// The player thread's state: one pipeline presenting into one surface.
pub(super) struct Session {
    gst: &'static Gst,
    events: Events,
    inbox: Arc<Inbox>,
    uri: String,
    pipeline: Option<Pipeline>,
    presenter: Presenter,
    tracks: Tracks,
    playing: bool,
    reported_playing: bool,
    looping: bool,
    rate: f64,
    volume: f64,
    muted: bool,
    loaded: bool,
    prerolled: bool,
    pending_seek: Option<(f64, bool)>,
    seeking: bool,
    buffering: bool,
    ended: bool,
    next_time: Option<Instant>,
    park_at: Option<Instant>,
}

impl Session {
    pub(super) fn new(
        gst: &'static Gst,
        options: PlayerOptions,
        presenter: Presenter,
        inbox: Arc<Inbox>,
        events: Events,
    ) -> Result<Self, String> {
        let mut session = Self {
            gst,
            events,
            inbox,
            uri: options.uri,
            pipeline: None,
            presenter,
            tracks: Tracks::default(),
            playing: options.autoplay,
            reported_playing: false,
            looping: options.looping,
            rate: options.rate,
            volume: options.volume,
            muted: options.muted,
            loaded: false,
            prerolled: false,
            pending_seek: None,
            seeking: false,
            buffering: false,
            ended: false,
            next_time: None,
            park_at: None,
        };
        session.unpark()?;
        Ok(session)
    }

    pub(super) fn run(mut self) {
        loop {
            let deadline = self.next_time.into_iter().chain(self.park_at).min();
            let mail = self.inbox.wait(deadline);
            if mail.stop {
                return;
            }
            for command in mail.commands {
                self.apply(command);
            }
            if mail.picture {
                self.take_picture();
                self.presenter.draw();
            }
            for message in mail.messages {
                self.handle(message);
            }
            for cue in mail.cues {
                self.cue(cue);
            }
            let now = Instant::now();
            if self.next_time.is_some_and(|deadline| deadline <= now) {
                self.report_time();
                self.next_time = Some(now + TIME_INTERVAL);
            }
            if self.park_at.is_some_and(|deadline| deadline <= now) {
                self.park();
            }
        }
    }

    fn unpark(&mut self) -> Result<(), String> {
        if self.pipeline.is_some() {
            return Ok(());
        }
        let pipeline = Pipeline::new(
            self.gst,
            &self.uri,
            self.presenter.gst_display(),
            self.presenter.gst_context(),
            &self.inbox,
        )?;
        pipeline.set_volume(self.volume);
        pipeline.set_muted(self.muted);
        pipeline.set_playing(false);
        self.pipeline = Some(pipeline);
        self.prerolled = false;
        Ok(())
    }

    fn park(&mut self) {
        self.park_at = None;
        if let Some(pipeline) = self.pipeline.take() {
            let time = pipeline.position().unwrap_or_default();
            self.pending_seek.get_or_insert((time, false));
        }
        self.prerolled = false;
        self.seeking = false;
    }

    fn resume(&mut self) {
        if let Err(message) = self.unpark() {
            self.emit("error", json!({ "message": message }));
        }
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Play => {
                self.playing = true;
                if self.ended {
                    self.seek(0.0, false);
                }
                self.resume();
                self.sync_state();
            }
            Command::Pause => {
                self.playing = false;
                self.sync_state();
            }
            Command::Seek { time, fast } => {
                self.resume();
                self.seek(time, fast);
            }
            Command::Rate(rate) => {
                self.rate = rate;
                if let Some(pipeline) = self.pipeline.as_ref().filter(|_| self.prerolled)
                    && !pipeline.change_rate(rate)
                {
                    let time = pipeline.position().unwrap_or_default();
                    self.seek(time, false);
                }
            }
            Command::Volume(volume) => {
                self.volume = volume;
                if let Some(pipeline) = &self.pipeline {
                    pipeline.set_volume(volume);
                }
            }
            Command::Muted(muted) => {
                self.muted = muted;
                if let Some(pipeline) = &self.pipeline {
                    pipeline.set_muted(muted);
                }
            }
            Command::Loop(looping) => self.looping = looping,
            Command::Tracks(request) => {
                if let (Some(selection), Some(pipeline)) =
                    (self.tracks.request(request), &self.pipeline)
                {
                    pipeline.select_streams(&selection);
                }
                if !self.tracks.subtitles_enabled() {
                    self.emit("cue", json!({ "text": "", "start": 0, "end": 0 }));
                }
            }
            Command::Frame(frame) => self.presenter.set_frame(frame),
            Command::Target(target) => self.presenter.set_target(target),
            Command::Occluded(occluded) => self.presenter.set_occluded(occluded),
        }
    }

    fn take_picture(&mut self) {
        let Some(pipeline) = &self.pipeline else {
            return;
        };
        if let Some((width, height)) = self.presenter.take_picture(pipeline.pictures) {
            self.emit("size", json!({ "width": width, "height": height }));
        }
    }

    fn handle(&mut self, message: Message) {
        let gst = self.gst;
        let handle = message.handle.0;
        let header = unsafe { &*(handle as *const types::MessageHeader) };
        match header.kind {
            types::MESSAGE_ASYNC_DONE => self.async_done(),
            types::MESSAGE_STATE_CHANGED => {
                let (mut old, mut new, mut pending) = (0, 0, 0);
                unsafe {
                    (gst.gst_message_parse_state_changed)(handle, &mut old, &mut new, &mut pending)
                };
                if pending == types::STATE_VOID_PENDING {
                    self.report_state(new == types::STATE_PLAYING);
                }
            }
            types::MESSAGE_DURATION_CHANGED => self.report_duration(),
            types::MESSAGE_EOS => {
                if self.looping {
                    self.seek(0.0, false);
                } else {
                    self.ended = true;
                    self.playing = false;
                    self.reported_playing = false;
                    self.next_time = None;
                    self.sync_state();
                    self.report_time();
                    self.emit("state", json!({ "state": "ended" }));
                }
            }
            types::MESSAGE_ERROR => {
                let mut error = std::ptr::null_mut();
                unsafe { (gst.gst_message_parse_error)(handle, &mut error, std::ptr::null_mut()) };
                let message = unsafe { gst::take_error(gst, error) };
                self.emit("error", json!({ "message": message }));
            }
            types::MESSAGE_BUFFERING => {
                let mut percent = 100;
                unsafe { (gst.gst_message_parse_buffering)(handle, &mut percent) };
                let buffering = percent < 100;
                if buffering != self.buffering {
                    self.buffering = buffering;
                    if buffering {
                        self.reported_playing = false;
                        self.emit("state", json!({ "state": "buffering" }));
                    } else if !self.playing {
                        self.emit("state", json!({ "state": "paused" }));
                    }
                    self.sync_state();
                }
            }
            types::MESSAGE_STREAM_COLLECTION => {
                let selection = unsafe { self.tracks.collect(gst, handle) };
                if let Some(pipeline) = &self.pipeline {
                    pipeline.select_streams(&selection);
                }
            }
            types::MESSAGE_STREAMS_SELECTED => {
                unsafe { self.tracks.selected(gst, handle) };
                self.emit("tracks", self.tracks.to_json());
            }
            _ => {}
        }
    }

    fn async_done(&mut self) {
        let Some(pipeline) = &self.pipeline else {
            return;
        };
        if !self.prerolled {
            self.prerolled = true;
            if !std::mem::replace(&mut self.loaded, true) {
                self.emit(
                    "loaded",
                    json!({
                        "duration": pipeline.duration(),
                        "time": pipeline.position().unwrap_or_default(),
                    }),
                );
            }
            match self.pending_seek.take() {
                Some((time, fast)) => self.seek(time, fast),
                None if self.rate != 1.0 => {
                    let time = pipeline.position().unwrap_or_default();
                    self.seek(time, false);
                }
                None => {}
            }
            self.sync_state();
            return;
        }
        if std::mem::take(&mut self.seeking) {
            let time = pipeline.position().unwrap_or_default();
            self.emit("seeked", json!({ "time": time }));
        }
    }

    fn seek(&mut self, time: f64, fast: bool) {
        self.ended = false;
        match self.pipeline.as_ref().filter(|_| self.prerolled) {
            Some(pipeline) => {
                if pipeline.seek(time, fast, self.rate) {
                    self.seeking = true;
                }
            }
            None => self.pending_seek = Some((time, fast)),
        }
    }

    fn sync_state(&mut self) {
        self.park_at = (!self.playing).then(|| Instant::now() + PARK_DELAY);
        if let Some(pipeline) = self.pipeline.as_ref().filter(|_| self.prerolled) {
            pipeline.set_playing(self.playing && !self.buffering);
        }
    }

    fn report_state(&mut self, playing: bool) {
        if playing == self.reported_playing || self.buffering || self.ended {
            return;
        }
        self.reported_playing = playing;
        self.next_time = playing.then(|| Instant::now() + TIME_INTERVAL);
        self.report_time();
        self.emit(
            "state",
            json!({ "state": if playing { "playing" } else { "paused" } }),
        );
    }

    fn report_time(&self) {
        if let Some(time) = self.pipeline.as_ref().and_then(Pipeline::position) {
            self.emit("time", json!({ "time": time }));
        }
    }

    fn report_duration(&self) {
        if let Some(duration) = self.pipeline.as_ref().and_then(Pipeline::duration) {
            self.emit("duration", json!({ "duration": duration }));
        }
    }

    fn cue(&self, cue: Cue) {
        if self.tracks.subtitles_enabled() {
            self.emit(
                "cue",
                json!({ "text": cue.text, "start": cue.start, "end": cue.end }),
            );
        }
    }

    fn emit(&self, kind: &str, mut payload: Value) {
        payload["type"] = kind.into();
        (self.events)(payload);
    }
}
