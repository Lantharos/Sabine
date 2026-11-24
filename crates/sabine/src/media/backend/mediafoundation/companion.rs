// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The Media Engine renders a single audio stream and has no volume per stream,
// so every further audio track the page mixes in plays through an audio-only
// engine of its own. Windows mixes their outputs; each companion follows the
// main engine's position and paused state, and is moved back when it drifts.

use std::sync::Arc;

use windows::Win32::Media::MediaFoundation::IMFMediaEngineEx;
use windows::core::BSTR;

use super::engine::Engine;
use super::streams;
use super::worker::Inbox;
use crate::media::command::AudioMix;

/// How far, in seconds, a companion may drift from the main engine.
const DRIFT: f64 = 0.05;

struct Companion {
    id: String,
    stream: u32,
    engine: Engine,
    ready: bool,
}

/// The audio-only engines playing every chosen audio track after the first.
pub(super) struct Companions {
    uri: String,
    inbox: Arc<Inbox>,
    playing: Vec<Companion>,
    settings: Settings,
}

pub(super) struct Settings {
    pub(super) volume: f64,
    pub(super) muted: bool,
    pub(super) rate: f64,
    pub(super) looping: bool,
}

impl Companions {
    pub(super) fn new(uri: String, inbox: Arc<Inbox>, settings: Settings) -> Self {
        Self {
            uri,
            inbox,
            playing: Vec::new(),
            settings,
        }
    }

    /// Starts companions for `extra` tracks, stops the ones no longer chosen
    /// and sets every companion's volume.
    pub(super) fn arrange(&mut self, extra: &[AudioMix]) -> Result<(), String> {
        self.playing
            .retain(|companion| extra.iter().any(|track| track.id == companion.id));
        for track in extra {
            if self
                .playing
                .iter()
                .all(|companion| companion.id != track.id)
                && let Some(stream) = streams::audio_stream(&track.id)
            {
                let companion = self.start(&track.id, stream)?;
                self.playing.push(companion);
            }
        }
        self.set_volume(self.settings.volume, extra);
        Ok(())
    }

    fn start(&self, id: &str, stream: u32) -> Result<Companion, String> {
        let engine = Engine::audio(&self.inbox, stream)?;
        let media = &engine.media;
        unsafe {
            media
                .SetAutoPlay(false)
                .and_then(|()| media.SetLoop(self.settings.looping))
                .and_then(|()| media.SetMuted(self.settings.muted))
                .and_then(|()| media.SetDefaultPlaybackRate(self.settings.rate))
                .and_then(|()| media.SetSource(&BSTR::from(self.uri.as_str())))
        }
        .map_err(|error| error.message())?;
        Ok(Companion {
            id: id.to_string(),
            stream,
            engine,
            ready: false,
        })
    }

    /// A companion knows its streams: it plays only its own and catches up.
    pub(super) fn loaded(&mut self, stream: u32, main: &IMFMediaEngineEx) {
        let Some(companion) = self
            .playing
            .iter_mut()
            .find(|companion| companion.stream == stream)
        else {
            return;
        };
        let media = &companion.engine.media;
        unsafe {
            for index in 0..media.GetNumberOfStreams().unwrap_or_default() {
                let _ = media.SetStreamSelection(index, index == stream);
            }
            let _ = media.ApplyStreamSelections();
        }
        companion.ready = true;
        companion.follow(main);
    }

    pub(super) fn follow(&self, main: &IMFMediaEngineEx) {
        for companion in &self.playing {
            companion.follow(main);
        }
    }

    pub(super) fn set_volume(&mut self, volume: f64, extra: &[AudioMix]) {
        self.settings.volume = volume;
        for companion in &self.playing {
            let track = extra.iter().find(|track| track.id == companion.id);
            let level = volume * track.map_or(0.0, |track| track.volume);
            let _ = unsafe { companion.engine.media.SetVolume(level) };
        }
    }

    pub(super) fn set_muted(&mut self, muted: bool) {
        self.settings.muted = muted;
        for companion in &self.playing {
            let _ = unsafe { companion.engine.media.SetMuted(muted) };
        }
    }

    pub(super) fn set_rate(&mut self, rate: f64) {
        self.settings.rate = rate;
        for companion in &self.playing {
            let media = &companion.engine.media;
            let _ = unsafe {
                media
                    .SetDefaultPlaybackRate(rate)
                    .and_then(|()| media.SetPlaybackRate(rate))
            };
        }
    }

    pub(super) fn set_loop(&mut self, looping: bool) {
        self.settings.looping = looping;
        for companion in &self.playing {
            let _ = unsafe { companion.engine.media.SetLoop(looping) };
        }
    }

    /// The tracks companions play.
    pub(super) fn ids(&self) -> impl Iterator<Item = String> + '_ {
        self.playing.iter().map(|companion| companion.id.clone())
    }
}

impl Companion {
    fn follow(&self, main: &IMFMediaEngineEx) {
        if !self.ready {
            return;
        }
        let media = &self.engine.media;
        unsafe {
            let time = main.GetCurrentTime();
            if (media.GetCurrentTime() - time).abs() > DRIFT {
                let _ = media.SetCurrentTime(time);
            }
            let paused = main.IsPaused().as_bool();
            if paused != media.IsPaused().as_bool() {
                let _ = if paused { media.Pause() } else { media.Play() };
            }
        }
    }
}
