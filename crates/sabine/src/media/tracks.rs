use serde_json::{Value, json};

use super::command::{AudioMix, TrackRequest};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Video,
    Audio,
    Subtitle,
}

pub(super) struct Track {
    pub(super) id: String,
    pub(super) kind: Kind,
    pub(super) language: Option<String>,
    pub(super) label: Option<String>,
    pub(super) codec: Option<String>,
    /// Whether the file marks this track as the one to play by default.
    pub(super) preferred: bool,
}

/// The tracks a file offers, which of them should play and which do.
///
/// Audio follows the file's default, or plays every track with `mix_audio`,
/// until the page picks the tracks to mix; subtitles stay off until the page
/// turns them on.
#[derive(Default)]
pub(super) struct Tracks {
    mix_audio: bool,
    tracks: Vec<Track>,
    audio: Vec<AudioMix>,
    subtitle: Option<String>,
    requested: TrackRequest,
    selected: Vec<String>,
}

impl Tracks {
    pub(super) fn new(mix_audio: bool) -> Self {
        Self {
            mix_audio,
            ..Self::default()
        }
    }

    pub(super) fn set(&mut self, tracks: Vec<Track>) {
        self.tracks = tracks;
        self.audio = self.mix(self.requested.audio.as_deref());
        self.subtitle = self.pick(
            Kind::Subtitle,
            self.requested.subtitle.clone().or(Some(None)),
        );
    }

    /// Applies the page's choice, returning whether the file's tracks are
    /// known so the choice can take effect.
    pub(super) fn request(&mut self, request: TrackRequest) -> bool {
        if let Some(audio) = request.audio {
            self.audio = self.mix(Some(&audio));
            self.requested.audio = Some(audio);
        }
        if let Some(subtitle) = request.subtitle {
            self.requested.subtitle = Some(subtitle.clone());
            self.subtitle = self.pick(Kind::Subtitle, Some(subtitle));
        }
        !self.tracks.is_empty()
    }

    /// Records which tracks are playing.
    pub(super) fn set_selected(&mut self, selected: Vec<String>) {
        self.selected = selected;
    }

    /// The audio tracks to play together, in the file's order.
    pub(super) fn audio(&self) -> &[AudioMix] {
        &self.audio
    }

    pub(super) fn subtitle(&self) -> Option<&str> {
        self.subtitle.as_deref()
    }

    pub(super) fn subtitles_enabled(&self) -> bool {
        self.subtitle().is_some()
    }

    pub(super) fn of(&self, kind: Kind) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(move |track| track.kind == kind)
    }

    pub(super) fn to_json(&self) -> Value {
        let list = |kind: Kind| {
            self.of(kind)
                .map(|track| {
                    let mut entry = json!({
                        "id": track.id,
                        "language": track.language,
                        "label": track.label,
                        "codec": track.codec,
                        "selected": self.selected.contains(&track.id),
                    });
                    if kind == Kind::Audio {
                        entry["volume"] = self.volume(&track.id).into();
                    }
                    entry
                })
                .collect::<Vec<_>>()
        };
        json!({
            "video": list(Kind::Video),
            "audio": list(Kind::Audio),
            "subtitles": list(Kind::Subtitle),
        })
    }

    fn volume(&self, id: &str) -> f64 {
        self.audio
            .iter()
            .find(|mix| mix.id == id)
            .map_or(1.0, |mix| mix.volume)
    }

    fn mix(&self, requested: Option<&[AudioMix]>) -> Vec<AudioMix> {
        let Some(requested) = requested else {
            let full = |id: String| AudioMix { id, volume: 1.0 };
            if self.mix_audio {
                return self
                    .of(Kind::Audio)
                    .map(|track| full(track.id.clone()))
                    .collect();
            }
            return self.pick(Kind::Audio, None).map(full).into_iter().collect();
        };
        self.of(Kind::Audio)
            .filter_map(|track| requested.iter().find(|mix| mix.id == track.id))
            .cloned()
            .collect()
    }

    fn pick(&self, kind: Kind, requested: Option<Option<String>>) -> Option<String> {
        let mut candidates = self.tracks.iter().filter(|track| track.kind == kind);
        match requested {
            Some(Some(id)) => candidates.find(|track| track.id == id),
            Some(None) => None,
            None => {
                let candidates = candidates.collect::<Vec<_>>();
                candidates
                    .iter()
                    .find(|track| track.preferred)
                    .or(candidates.first())
                    .copied()
            }
        }
        .map(|track| track.id.clone())
    }
}
