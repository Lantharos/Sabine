use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::{MainThreadMarker, rc::Retained, runtime::AnyObject, runtime::Bool, sel};
use objc2_av_foundation::{
    AVMediaSelectionGroup, AVPlayer, AVPlayerActionAtItemEnd, AVPlayerItem,
    AVPlayerItemDidPlayToEndTimeNotification, AVPlayerItemLegibleOutput,
    AVPlayerItemMediaSelectionDidChangeNotification, AVPlayerItemStatus, AVPlayerTimeControlStatus,
    AVPlayerWaitingToMinimizeStallsReason,
};
use objc2_core_media::{CMTime, kCMTimePositiveInfinity, kCMTimeZero};
use objc2_foundation::{
    NSKeyValueObservingOptions, NSNotificationCenter, NSObjectNSKeyValueObserverRegistration,
    NSString, NSURL,
};
use serde_json::{Value, json};

use super::companion::Companions;
use super::observer::{Observer, Target};
use super::selection::{self, Group, Groups};
use crate::media::command::{self, Command, Events, PlayerOptions};
use crate::media::tracks::Tracks;

const TIME_INTERVAL: f64 = 0.25;
const TIMESCALE: i32 = 600;

/// A property of the player or its item that changed.
#[derive(Clone, Copy)]
pub(super) enum Change {
    Status,
    TimeControl,
    Duration,
    Size,
}

impl Change {
    const OBSERVED: [(Self, &str); 4] = [
        (Self::Status, "status"),
        (Self::TimeControl, "timeControlStatus"),
        (Self::Duration, "duration"),
        (Self::Size, "presentationSize"),
    ];

    pub(super) fn from_key_path(path: &str) -> Option<Self> {
        Self::OBSERVED
            .iter()
            .find(|(_, key)| *key == path)
            .map(|(change, _)| *change)
    }

    fn on_player(self) -> bool {
        matches!(self, Self::TimeControl)
    }
}

/// One `AVPlayer` and the page state it reports, living on the main thread.
pub(super) struct Playback {
    target: Target,
    pub(super) player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    observer: Retained<Observer>,
    legible: Retained<AVPlayerItemLegibleOutput>,
    clock: Retained<AnyObject>,
    events: Events,
    main_thread: MainThreadMarker,
    state: RefCell<State>,
}

struct State {
    loaded: bool,
    playing: bool,
    reported_playing: bool,
    buffering: bool,
    ended: bool,
    looping: bool,
    rate: f64,
    volume: f64,
    tracks: Tracks,
    groups: Groups,
    companions: Companions,
}

impl Playback {
    pub(super) fn new(
        options: PlayerOptions,
        events: Events,
        main_thread: MainThreadMarker,
    ) -> Result<Rc<Self>, String> {
        let url = NSURL::URLWithString(&NSString::from_str(&options.uri))
            .ok_or_else(|| format!("{} is not a valid URL", options.uri))?;
        let item = unsafe { AVPlayerItem::playerItemWithURL(&url, main_thread) };
        let player = unsafe { AVPlayer::playerWithPlayerItem(Some(&item), main_thread) };
        unsafe {
            player.setActionAtItemEnd(AVPlayerActionAtItemEnd::None);
            player.setAppliesMediaSelectionCriteriaAutomatically(false);
            player.setVolume(options.volume as f32);
            player.setMuted(options.muted);
        }
        let playback = Rc::new_cyclic(|weak: &Weak<Self>| {
            let target = Target::register(weak.clone(), main_thread);
            let observer = Observer::new(target);
            let legible = unsafe { AVPlayerItemLegibleOutput::new() };
            let tick = RcBlock::new(move |_: CMTime| target.deliver(Self::tick));
            let clock = unsafe {
                player.addPeriodicTimeObserverForInterval_queue_usingBlock(
                    CMTime::with_seconds(TIME_INTERVAL, TIMESCALE),
                    Some(DispatchQueue::main()),
                    &tick,
                )
            };
            Self {
                target,
                player,
                item,
                observer,
                legible,
                clock,
                events,
                main_thread,
                state: RefCell::new(State {
                    loaded: false,
                    playing: options.autoplay,
                    reported_playing: false,
                    buffering: false,
                    ended: false,
                    looping: options.looping,
                    rate: options.rate,
                    volume: options.volume,
                    tracks: Tracks::new(options.mix_audio),
                    groups: Groups::default(),
                    companions: Companions::new(options.volume, options.muted),
                }),
            }
        });
        playback.observe();
        if options.autoplay {
            unsafe { playback.player.setRate(options.rate as f32) };
        }
        Ok(playback)
    }

    fn observe(&self) {
        unsafe {
            self.legible.setSuppressesPlayerRendering(true);
            self.legible.setDelegate_queue(
                Some(objc2::runtime::ProtocolObject::from_ref(&*self.observer)),
                Some(DispatchQueue::main()),
            );
            self.item.addOutput(&self.legible);
            for (change, key) in Change::OBSERVED {
                let object = if change.on_player() {
                    &**self.player
                } else {
                    &**self.item
                };
                object.addObserver_forKeyPath_options_context(
                    &self.observer,
                    &NSString::from_str(key),
                    NSKeyValueObservingOptions::New,
                    std::ptr::null_mut(),
                );
            }
            let center = NSNotificationCenter::defaultCenter();
            center.addObserver_selector_name_object(
                &self.observer,
                sel!(itemEnded:),
                Some(AVPlayerItemDidPlayToEndTimeNotification),
                Some(&self.item),
            );
            center.addObserver_selector_name_object(
                &self.observer,
                sel!(selectionChanged:),
                Some(AVPlayerItemMediaSelectionDidChangeNotification),
                Some(&self.item),
            );
        }
    }

    pub(super) fn apply(&self, command: Command) {
        match command {
            Command::Play => {
                let (ended, rate) = {
                    let mut state = self.state.borrow_mut();
                    state.playing = true;
                    (std::mem::take(&mut state.ended), state.rate)
                };
                if ended {
                    self.seek(0.0, false);
                }
                unsafe { self.player.setRate(rate as f32) };
                self.follow();
            }
            Command::Pause => {
                self.state.borrow_mut().playing = false;
                unsafe { self.player.pause() };
                self.follow();
            }
            Command::Seek { time, fast } => self.seek(time, fast),
            Command::Rate(rate) => {
                let playing = {
                    let mut state = self.state.borrow_mut();
                    state.rate = rate;
                    state.playing
                };
                if playing {
                    unsafe { self.player.setRate(rate as f32) };
                    self.follow();
                }
            }
            Command::Volume(volume) => {
                let mut state = self.state.borrow_mut();
                state.volume = volume;
                self.apply_volume(&mut state);
            }
            Command::Muted(muted) => {
                unsafe { self.player.setMuted(muted) };
                self.state.borrow_mut().companions.set_muted(muted);
            }
            Command::Loop(looping) => self.state.borrow_mut().looping = looping,
            Command::Tracks(request) => {
                let (known, subtitles) = {
                    let mut state = self.state.borrow_mut();
                    let known = state.tracks.request(request);
                    if known {
                        self.select_tracks(&mut state);
                    }
                    (known, state.tracks.subtitles_enabled())
                };
                if known {
                    self.follow();
                    self.selection_changed();
                }
                if !subtitles {
                    self.emit("cue", json!({ "text": "", "start": 0, "end": null }));
                }
            }
        }
    }

    fn select_tracks(&self, state: &mut State) {
        selection::apply(&self.item, &state.groups, &state.tracks);
        let extra = state.tracks.audio().get(1..).unwrap_or_default();
        state
            .companions
            .arrange(&self.item, &state.groups, extra, self.main_thread);
        self.apply_volume(state);
    }

    fn apply_volume(&self, state: &mut State) {
        let audio = state.tracks.audio();
        let first = audio.first().map_or(1.0, |track| track.volume);
        unsafe { self.player.setVolume((state.volume * first) as f32) };
        let extra = audio.get(1..).unwrap_or_default();
        state.companions.set_volume(state.volume, extra);
    }

    fn follow(&self) {
        self.state.borrow().companions.follow(&self.player);
    }

    fn seek(&self, time: f64, fast: bool) {
        self.state.borrow_mut().ended = false;
        let tolerance = if fast {
            unsafe { kCMTimePositiveInfinity }
        } else {
            unsafe { kCMTimeZero }
        };
        let target = self.target;
        let done = RcBlock::new(move |finished: Bool| {
            if finished.as_bool() {
                target.deliver(Self::seeked);
            }
        });
        unsafe {
            self.player
                .seekToTime_toleranceBefore_toleranceAfter_completionHandler(
                    CMTime::with_seconds(time.max(0.0), TIMESCALE),
                    tolerance,
                    tolerance,
                    &done,
                );
        }
    }

    pub(super) fn changed(&self, change: Change) {
        match change {
            Change::Status => self.status_changed(),
            Change::TimeControl => self.time_control_changed(),
            Change::Duration => {
                if let Some(duration) = self.duration() {
                    self.emit("duration", json!({ "duration": duration }));
                }
            }
            Change::Size => {
                let size = unsafe { self.item.presentationSize() };
                if size.width > 0.0 && size.height > 0.0 {
                    self.emit(
                        "size",
                        json!({ "width": size.width, "height": size.height }),
                    );
                }
            }
        }
    }

    fn status_changed(&self) {
        match unsafe { self.item.status() } {
            AVPlayerItemStatus::ReadyToPlay => {
                if std::mem::replace(&mut self.state.borrow_mut().loaded, true) {
                    return;
                }
                self.emit(
                    "loaded",
                    json!({ "duration": self.duration(), "time": self.time().unwrap_or_default() }),
                );
                selection::load(&self.item, self.target);
            }
            AVPlayerItemStatus::Failed => {
                let message = unsafe { self.item.error() }.map_or_else(
                    || "the video could not be played".to_string(),
                    |error| error.localizedDescription().to_string(),
                );
                self.emit("error", json!({ "message": message }));
            }
            _ => {}
        }
    }

    fn time_control_changed(&self) {
        let status = unsafe { self.player.timeControlStatus() };
        if status == AVPlayerTimeControlStatus::WaitingToPlayAtSpecifiedRate {
            let stalled = unsafe { self.player.reasonForWaitingToPlay() }
                .is_some_and(|reason| &*reason == unsafe { AVPlayerWaitingToMinimizeStallsReason });
            let mut state = self.state.borrow_mut();
            if stalled && !std::mem::replace(&mut state.buffering, true) {
                state.reported_playing = false;
                drop(state);
                self.emit("state", json!({ "state": "buffering" }));
            }
            return;
        }
        self.state.borrow_mut().buffering = false;
        self.follow();
        self.report(status == AVPlayerTimeControlStatus::Playing);
    }

    fn report(&self, playing: bool) {
        {
            let mut state = self.state.borrow_mut();
            if playing == state.reported_playing || state.ended {
                return;
            }
            state.reported_playing = playing;
        }
        self.report_time();
        self.emit(
            "state",
            json!({ "state": if playing { "playing" } else { "paused" } }),
        );
    }

    fn tick(&self) {
        if self.state.borrow().reported_playing {
            self.report_time();
            self.follow();
        }
    }

    pub(super) fn ended(&self) {
        let mut state = self.state.borrow_mut();
        if state.looping {
            drop(state);
            self.seek(0.0, false);
            return;
        }
        state.ended = true;
        state.playing = false;
        state.reported_playing = false;
        drop(state);
        unsafe { self.player.pause() };
        self.follow();
        self.report_time();
        self.emit("state", json!({ "state": "ended" }));
    }

    fn seeked(&self) {
        self.follow();
        if let Some(time) = self.time() {
            self.emit("seeked", json!({ "time": time }));
        }
    }

    pub(super) fn group_loaded(
        &self,
        group: Group,
        loaded: Option<Retained<AVMediaSelectionGroup>>,
    ) {
        let mut state = self.state.borrow_mut();
        if !state.groups.insert(group, loaded) {
            return;
        }
        let tracks = selection::tracks(&self.item, &state.groups);
        state.tracks.set(tracks);
        self.select_tracks(&mut state);
        drop(state);
        self.follow();
        self.selection_changed();
    }

    pub(super) fn selection_changed(&self) {
        let mut state = self.state.borrow_mut();
        let mut selected = selection::selected(&self.item, &state.groups);
        selected.extend(state.companions.ids());
        state.tracks.set_selected(selected);
        let tracks = state.tracks.to_json();
        drop(state);
        self.emit("tracks", tracks);
    }

    pub(super) fn cue(&self, text: String, start: f64) {
        if self.state.borrow().tracks.subtitles_enabled() {
            self.emit("cue", json!({ "text": text, "start": start, "end": null }));
        }
    }

    fn time(&self) -> Option<f64> {
        let time = unsafe { self.player.currentTime().seconds() };
        time.is_finite().then_some(time.max(0.0))
    }

    fn duration(&self) -> Option<f64> {
        let duration = unsafe { self.item.duration().seconds() };
        (duration.is_finite() && duration > 0.0).then_some(duration)
    }

    fn report_time(&self) {
        if let Some(time) = self.time() {
            self.emit("time", json!({ "time": time }));
        }
    }

    fn emit(&self, kind: &str, payload: Value) {
        command::emit(&self.events, kind, payload);
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.target.unregister();
        unsafe {
            NSNotificationCenter::defaultCenter().removeObserver(&self.observer);
            for (change, key) in Change::OBSERVED {
                let object = if change.on_player() {
                    &**self.player
                } else {
                    &**self.item
                };
                object.removeObserver_forKeyPath(&self.observer, &NSString::from_str(key));
            }
            self.player.removeTimeObserver(&self.clock);
            self.item.removeOutput(&self.legible);
            self.player.pause();
            self.player.replaceCurrentItemWithPlayerItem(None);
        }
    }
}
