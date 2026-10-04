// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The engine owns its windowless swapchain and only has one once it knows the
// video, so the swapchain is bound to the player's visual whenever both exist.
// Its size follows the page element in physical pixels.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use windows::Win32::{
    Foundation::RECT,
    Media::MediaFoundation::{
        MF_MEDIA_ENGINE_ERR, MF_MEDIA_ENGINE_ERR_ABORTED, MF_MEDIA_ENGINE_ERR_DECODE,
        MF_MEDIA_ENGINE_ERR_NETWORK, MF_MEDIA_ENGINE_ERR_SRC_NOT_SUPPORTED, MF_MEDIA_ENGINE_EVENT,
        MF_MEDIA_ENGINE_EVENT_BUFFERINGENDED, MF_MEDIA_ENGINE_EVENT_BUFFERINGSTARTED,
        MF_MEDIA_ENGINE_EVENT_DURATIONCHANGE, MF_MEDIA_ENGINE_EVENT_ENDED,
        MF_MEDIA_ENGINE_EVENT_ERROR, MF_MEDIA_ENGINE_EVENT_FIRSTFRAMEREADY,
        MF_MEDIA_ENGINE_EVENT_FORMATCHANGE, MF_MEDIA_ENGINE_EVENT_LOADEDDATA,
        MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA, MF_MEDIA_ENGINE_EVENT_PAUSE,
        MF_MEDIA_ENGINE_EVENT_PLAYING, MF_MEDIA_ENGINE_EVENT_SEEKED,
        MF_MEDIA_ENGINE_EVENT_TRACKSCHANGE, MF_MEDIA_ENGINE_EVENT_WAITING,
        MF_MEDIA_ENGINE_SEEK_MODE_APPROXIMATE, MFARGB,
    },
    System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
};
use windows::core::{BSTR, HRESULT};

use super::engine::Engine;
use super::streams;
use super::worker::{Cue, Inbox, Order, Report, Surface};
use crate::media::command::{self, Command, Events, PlayerOptions};
use crate::media::geometry::Frame;
use crate::media::tracks::Tracks;

const TIME_INTERVAL: Duration = Duration::from_millis(250);
const BORDER: MFARGB = MFARGB {
    rgbBlue: 0,
    rgbGreen: 0,
    rgbRed: 0,
    rgbAlpha: 255,
};

/// The player thread's state: one Media Engine presenting into one visual.
pub(super) struct Session {
    engine: Engine,
    events: Events,
    inbox: Arc<Inbox>,
    surface: Option<Surface>,
    bound: bool,
    frame: Option<Frame>,
    tracks: Tracks,
    loaded: bool,
    reported_playing: bool,
    buffering: bool,
    cue: Option<u32>,
    next_time: Option<Instant>,
    _apartment: Apartment,
}

/// Keeps the thread in the multithreaded COM apartment the engine lives in.
struct Apartment;

impl Apartment {
    fn enter() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map(|()| Self)
            .map_err(|error| error.message())
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

impl Session {
    pub(super) fn new(
        options: PlayerOptions,
        events: Events,
        inbox: Arc<Inbox>,
    ) -> Result<Self, String> {
        let apartment = Apartment::enter()?;
        let engine = Engine::new(&inbox)?;
        let media = &engine.media;
        unsafe {
            media
                .SetAutoPlay(options.autoplay)
                .map_err(|error| error.message())?;
            media
                .SetLoop(options.looping)
                .map_err(|error| error.message())?;
            media
                .SetVolume(options.volume.clamp(0.0, 1.0))
                .map_err(|error| error.message())?;
            media
                .SetMuted(options.muted)
                .map_err(|error| error.message())?;
            media
                .SetDefaultPlaybackRate(options.rate)
                .map_err(|error| error.message())?;
            media
                .SetSource(&BSTR::from(options.uri.as_str()))
                .map_err(|error| error.message())?;
        }
        Ok(Self {
            engine,
            events,
            inbox,
            surface: None,
            bound: false,
            frame: None,
            tracks: Tracks::default(),
            loaded: false,
            reported_playing: false,
            buffering: false,
            cue: None,
            next_time: None,
            _apartment: apartment,
        })
    }

    pub(super) fn run(mut self) {
        loop {
            let mail = self.inbox.wait(self.next_time);
            if mail.stop {
                return;
            }
            for order in mail.orders {
                self.order(order);
            }
            for report in mail.reports {
                self.report(report);
            }
            if self
                .next_time
                .is_some_and(|deadline| deadline <= Instant::now())
            {
                self.report_time();
                self.next_time = Some(Instant::now() + TIME_INTERVAL);
            }
        }
    }

    fn order(&mut self, order: Order) {
        match order {
            Order::Page(command) => self.apply(command),
            Order::Frame(frame) => {
                self.frame = frame;
                self.resize();
            }
            Order::Surface(surface) => {
                self.surface = surface;
                self.bound = false;
                self.bind();
            }
        }
    }

    fn apply(&mut self, command: Command) {
        if let Command::Tracks(request) = command {
            if self.tracks.request(request) {
                streams::apply(&self.engine.media, self.engine.text.as_ref(), &self.tracks);
                self.report_tracks();
            }
            if !self.tracks.subtitles_enabled() {
                self.clear_cue();
            }
            return;
        }
        let media = &self.engine.media;
        let result = unsafe {
            match command {
                Command::Play => media.Play(),
                Command::Pause => media.Pause(),
                Command::Seek { time, fast: true } => {
                    media.SetCurrentTimeEx(time.max(0.0), MF_MEDIA_ENGINE_SEEK_MODE_APPROXIMATE)
                }
                Command::Seek { time, fast: false } => media.SetCurrentTime(time.max(0.0)),
                Command::Rate(rate) => media
                    .SetDefaultPlaybackRate(rate)
                    .and_then(|()| media.SetPlaybackRate(rate)),
                Command::Volume(volume) => media.SetVolume(volume.clamp(0.0, 1.0)),
                Command::Muted(muted) => media.SetMuted(muted),
                Command::Loop(looping) => media.SetLoop(looping),
                Command::Tracks(_) => Ok(()),
            }
        };
        if let Err(error) = result {
            self.emit("error", json!({ "message": error.message() }));
        }
    }

    fn report(&mut self, report: Report) {
        match report {
            Report::Engine {
                event,
                code,
                result,
            } => self.engine_event(MF_MEDIA_ENGINE_EVENT(event as i32), code, result),
            Report::TextTracks => self.read_tracks(),
            Report::Cue(cue) => self.show_cue(cue),
            Report::CueEnded(id) => {
                if self.cue == Some(id) {
                    self.clear_cue();
                }
            }
            Report::CuesCleared => self.clear_cue(),
        }
    }

    fn engine_event(&mut self, event: MF_MEDIA_ENGINE_EVENT, code: usize, result: u32) {
        match event {
            MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA => {
                self.read_tracks();
                self.report_size();
                self.bind();
            }
            MF_MEDIA_ENGINE_EVENT_LOADEDDATA => {
                if !std::mem::replace(&mut self.loaded, true) {
                    self.emit(
                        "loaded",
                        json!({ "duration": self.duration(), "time": self.time() }),
                    );
                }
            }
            MF_MEDIA_ENGINE_EVENT_FORMATCHANGE | MF_MEDIA_ENGINE_EVENT_FIRSTFRAMEREADY => {
                self.report_size();
                self.bind();
            }
            MF_MEDIA_ENGINE_EVENT_TRACKSCHANGE => self.read_tracks(),
            MF_MEDIA_ENGINE_EVENT_DURATIONCHANGE => {
                if let Some(duration) = self.duration() {
                    self.emit("duration", json!({ "duration": duration }));
                }
            }
            MF_MEDIA_ENGINE_EVENT_PLAYING => {
                self.buffering = false;
                self.report_state(true);
            }
            MF_MEDIA_ENGINE_EVENT_PAUSE => self.report_state(false),
            MF_MEDIA_ENGINE_EVENT_WAITING | MF_MEDIA_ENGINE_EVENT_BUFFERINGSTARTED => {
                if !std::mem::replace(&mut self.buffering, true) {
                    self.reported_playing = false;
                    self.next_time = None;
                    self.emit("state", json!({ "state": "buffering" }));
                }
            }
            MF_MEDIA_ENGINE_EVENT_BUFFERINGENDED => self.buffering = false,
            MF_MEDIA_ENGINE_EVENT_SEEKED => {
                self.emit("seeked", json!({ "time": self.time() }));
            }
            MF_MEDIA_ENGINE_EVENT_ENDED => {
                self.reported_playing = false;
                self.next_time = None;
                self.report_time();
                self.emit("state", json!({ "state": "ended" }));
            }
            MF_MEDIA_ENGINE_EVENT_ERROR => {
                let message = error_message(MF_MEDIA_ENGINE_ERR(code as i32), result);
                self.emit("error", json!({ "message": message }));
            }
            _ => {}
        }
    }

    /// Shows the engine's swapchain in the player's visual once both exist.
    fn bind(&mut self) {
        let Some(surface) = self.surface.as_ref().filter(|_| !self.bound) else {
            return;
        };
        self.resize();
        let Ok(handle) = (unsafe { self.engine.media.GetVideoSwapchainHandle() }) else {
            return;
        };
        let bound = unsafe {
            surface
                .composition
                .device()
                .CreateSurfaceFromHandle(handle)
                .and_then(|content| surface.visual.SetContent(&content))
        };
        match bound {
            Ok(()) => {
                surface.composition.commit();
                self.bound = true;
            }
            Err(error) => self.emit("error", json!({ "message": error.message() })),
        }
    }

    /// Sizes the swapchain to the element; the engine letterboxes inside it.
    fn resize(&self) {
        let Some(frame) = self.frame else {
            return;
        };
        let [_, _, width, height] = frame.video;
        let target = RECT {
            left: 0,
            top: 0,
            right: width.round().max(1.0) as i32,
            bottom: height.round().max(1.0) as i32,
        };
        let _ = unsafe {
            self.engine
                .media
                .UpdateVideoStream(None, Some(&target), Some(&BORDER))
        };
    }

    fn read_tracks(&mut self) {
        let tracks = streams::tracks(&self.engine.media, self.engine.text.as_ref());
        self.tracks.set(tracks);
        streams::apply(&self.engine.media, self.engine.text.as_ref(), &self.tracks);
        self.report_tracks();
    }

    fn report_tracks(&mut self) {
        let selected = streams::selected(&self.engine.media, &self.tracks);
        self.tracks.set_selected(selected);
        self.emit("tracks", self.tracks.to_json());
    }

    fn report_size(&self) {
        let media = &self.engine.media;
        let (mut width, mut height, mut aspect_x, mut aspect_y) = (0, 0, 1, 1);
        let known = unsafe {
            media
                .GetNativeVideoSize(Some(&mut width), Some(&mut height))
                .is_ok()
                && media
                    .GetVideoAspectRatio(Some(&mut aspect_x), Some(&mut aspect_y))
                    .is_ok()
        };
        if known && width > 0 && height > 0 && aspect_y > 0 {
            self.emit(
                "size",
                json!({
                    "width": f64::from(width) * f64::from(aspect_x) / f64::from(aspect_y),
                    "height": height,
                }),
            );
        }
    }

    fn report_state(&mut self, playing: bool) {
        if playing == self.reported_playing || self.buffering {
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
        self.emit("time", json!({ "time": self.time() }));
    }

    fn time(&self) -> f64 {
        unsafe { self.engine.media.GetCurrentTime() }
    }

    fn duration(&self) -> Option<f64> {
        let duration = unsafe { self.engine.media.GetDuration() };
        (duration.is_finite() && duration > 0.0).then_some(duration)
    }

    fn show_cue(&mut self, cue: Cue) {
        if self.tracks.subtitles_enabled() {
            self.cue = Some(cue.id);
            self.emit(
                "cue",
                json!({ "text": cue.text, "start": cue.start, "end": cue.end }),
            );
        }
    }

    fn clear_cue(&mut self) {
        self.cue = None;
        self.emit("cue", json!({ "text": "", "start": 0, "end": 0 }));
    }

    fn emit(&self, kind: &str, payload: Value) {
        command::emit(&self.events, kind, payload);
    }
}

fn error_message(code: MF_MEDIA_ENGINE_ERR, result: u32) -> String {
    let reason = match code {
        MF_MEDIA_ENGINE_ERR_ABORTED => "loading the video was aborted",
        MF_MEDIA_ENGINE_ERR_NETWORK => "the video could not be downloaded",
        MF_MEDIA_ENGINE_ERR_DECODE => "the video could not be decoded",
        MF_MEDIA_ENGINE_ERR_SRC_NOT_SUPPORTED => "the video format is not supported",
        _ => "the video could not be played",
    };
    let detail = HRESULT(result as i32).message();
    if detail.is_empty() {
        reason.to_string()
    } else {
        format!("{reason}: {detail}")
    }
}
