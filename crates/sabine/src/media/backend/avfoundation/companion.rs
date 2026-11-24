//! An item plays one option of its audible group, so every further audio
//! track the page mixes in plays through a player of its own with no layer,
//! which never draws video. Each companion follows the main player's
//! position and rate, and is moved back when it drifts.

use objc2::{MainThreadMarker, rc::Retained};
use objc2_av_foundation::{AVPlayer, AVPlayerActionAtItemEnd, AVPlayerItem};
use objc2_core_media::{CMTime, kCMTimeZero};

use super::selection::{self, Groups};
use crate::media::command::AudioMix;

/// How far, in seconds, a companion may drift from the main player.
const DRIFT: f64 = 0.05;
const TIMESCALE: i32 = 600;

struct Companion {
    id: String,
    player: Retained<AVPlayer>,
}

/// The players playing every chosen audio track after the first.
pub(super) struct Companions {
    playing: Vec<Companion>,
    volume: f64,
    muted: bool,
}

impl Companions {
    pub(super) fn new(volume: f64, muted: bool) -> Self {
        Self {
            playing: Vec::new(),
            volume,
            muted,
        }
    }

    /// Starts companions for `extra` tracks, stops the ones no longer chosen
    /// and sets every companion's volume.
    pub(super) fn arrange(
        &mut self,
        main: &AVPlayerItem,
        groups: &Groups,
        extra: &[AudioMix],
        main_thread: MainThreadMarker,
    ) {
        self.playing
            .retain(|companion| extra.iter().any(|track| track.id == companion.id));
        for track in extra {
            if self
                .playing
                .iter()
                .all(|companion| companion.id != track.id)
                && let Some((group, option)) = selection::audio_option(groups, &track.id)
            {
                let item = unsafe { AVPlayerItem::playerItemWithAsset(&main.asset(), main_thread) };
                unsafe { item.selectMediaOption_inMediaSelectionGroup(Some(&option), group) };
                let player = unsafe { AVPlayer::playerWithPlayerItem(Some(&item), main_thread) };
                unsafe {
                    player.setActionAtItemEnd(AVPlayerActionAtItemEnd::None);
                    player.setAppliesMediaSelectionCriteriaAutomatically(false);
                    player.setMuted(self.muted);
                }
                self.playing.push(Companion {
                    id: track.id.clone(),
                    player,
                });
            }
        }
        self.set_volume(self.volume, extra);
    }

    pub(super) fn follow(&self, main: &AVPlayer) {
        let time = unsafe { main.currentTime().seconds() };
        let rate = unsafe { main.rate() };
        for companion in &self.playing {
            let player = &companion.player;
            let own = unsafe { player.currentTime().seconds() };
            if time.is_finite() && (own - time).abs() > DRIFT {
                unsafe {
                    player.seekToTime_toleranceBefore_toleranceAfter(
                        CMTime::with_seconds(time, TIMESCALE),
                        kCMTimeZero,
                        kCMTimeZero,
                    )
                };
            }
            if unsafe { player.rate() } != rate {
                unsafe { player.setRate(rate) };
            }
        }
    }

    pub(super) fn set_volume(&mut self, volume: f64, extra: &[AudioMix]) {
        self.volume = volume;
        for companion in &self.playing {
            let track = extra.iter().find(|track| track.id == companion.id);
            let level = volume * track.map_or(0.0, |track| track.volume);
            unsafe { companion.player.setVolume(level as f32) };
        }
    }

    pub(super) fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        for companion in &self.playing {
            unsafe { companion.player.setMuted(muted) };
        }
    }

    /// The tracks companions play.
    pub(super) fn ids(&self) -> impl Iterator<Item = String> + '_ {
        self.playing.iter().map(|companion| companion.id.clone())
    }
}

impl Drop for Companion {
    fn drop(&mut self) {
        unsafe {
            self.player.pause();
            self.player.replaceCurrentItemWithPlayerItem(None);
        }
    }
}
