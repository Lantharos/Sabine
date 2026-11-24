use sabine_bridge::media as commands;
use serde::Deserialize;
use serde_json::Value;

use super::command::{AudioMix, Command, TrackRequest};
use super::geometry::{PageRect, Rect};

pub(super) enum Request {
    Create(Create),
    Destroy(u64),
    SetRect(u64, Option<PageRect>),
    Command(u64, Command),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Create {
    pub(super) src: String,
    #[serde(default)]
    pub(super) autoplay: bool,
    #[serde(default, rename = "loop")]
    pub(super) looping: bool,
    #[serde(default = "one")]
    pub(super) volume: f64,
    #[serde(default)]
    pub(super) muted: bool,
    #[serde(default = "one")]
    pub(super) rate: f64,
    #[serde(default)]
    pub(super) mix_audio: bool,
}

#[derive(Deserialize)]
struct Placement {
    id: u64,
    #[serde(flatten)]
    bounds: Rect,
    #[serde(default)]
    radius: f64,
    clip: Option<Rect>,
    #[serde(default = "shown")]
    visible: bool,
}

#[derive(Deserialize)]
struct Selection {
    id: u64,
    audio: Option<Vec<AudioTrack>>,
    #[serde(default, deserialize_with = "present")]
    subtitle: Option<Option<String>>,
}

#[derive(Deserialize)]
struct AudioTrack {
    id: String,
    #[serde(default = "one")]
    volume: f64,
}

/// Tells a field set to `null` apart from a missing one.
fn present<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
struct Target {
    id: u64,
}

#[derive(Deserialize)]
struct Seek {
    id: u64,
    time: f64,
    #[serde(default)]
    fast: bool,
}

#[derive(Deserialize)]
struct Number {
    id: u64,
    #[serde(alias = "rate", alias = "volume")]
    value: f64,
}

#[derive(Deserialize)]
struct Flag {
    id: u64,
    #[serde(alias = "muted", alias = "loop")]
    value: bool,
}

fn one() -> f64 {
    1.0
}

fn shown() -> bool {
    true
}

fn rate(value: f64) -> Result<f64, String> {
    if value > 0.0 && value <= 16.0 {
        Ok(value)
    } else {
        Err("playback rate must be above 0 and at most 16".to_string())
    }
}

fn volume(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

impl Request {
    pub(super) fn parse(command: &str, params: Value) -> Result<Self, String> {
        let invalid = |error: serde_json::Error| format!("invalid {command} request: {error}");
        let id = |params: Value| {
            serde_json::from_value::<Target>(params)
                .map(|target| target.id)
                .map_err(invalid)
        };
        Ok(match command {
            commands::CREATE_COMMAND => {
                let create: Create = serde_json::from_value(params).map_err(invalid)?;
                Self::Create(Create {
                    rate: rate(create.rate)?,
                    volume: volume(create.volume),
                    ..create
                })
            }
            commands::DESTROY_COMMAND => Self::Destroy(id(params)?),
            commands::SET_RECT_COMMAND => {
                let placement: Placement = serde_json::from_value(params).map_err(invalid)?;
                let rect = (placement.visible
                    && placement.bounds.width > 0.0
                    && placement.bounds.height > 0.0)
                    .then_some(PageRect {
                        bounds: placement.bounds,
                        clip: placement.clip,
                        radius: placement.radius.max(0.0),
                    });
                Self::SetRect(placement.id, rect)
            }
            commands::PLAY_COMMAND => Self::Command(id(params)?, Command::Play),
            commands::PAUSE_COMMAND => Self::Command(id(params)?, Command::Pause),
            commands::SEEK_COMMAND => {
                let seek: Seek = serde_json::from_value(params).map_err(invalid)?;
                Self::Command(
                    seek.id,
                    Command::Seek {
                        time: seek.time,
                        fast: seek.fast,
                    },
                )
            }
            commands::SET_RATE_COMMAND => {
                let number: Number = serde_json::from_value(params).map_err(invalid)?;
                Self::Command(number.id, Command::Rate(rate(number.value)?))
            }
            commands::SET_VOLUME_COMMAND => {
                let number: Number = serde_json::from_value(params).map_err(invalid)?;
                Self::Command(number.id, Command::Volume(volume(number.value)))
            }
            commands::SET_MUTED_COMMAND => {
                let muted: Flag = serde_json::from_value(params).map_err(invalid)?;
                Self::Command(muted.id, Command::Muted(muted.value))
            }
            commands::SET_LOOP_COMMAND => {
                let looping: Flag = serde_json::from_value(params).map_err(invalid)?;
                Self::Command(looping.id, Command::Loop(looping.value))
            }
            commands::SELECT_TRACKS_COMMAND => {
                let selection: Selection = serde_json::from_value(params).map_err(invalid)?;
                let audio = selection.audio.map(|tracks| {
                    tracks
                        .into_iter()
                        .map(|track| AudioMix {
                            id: track.id,
                            volume: volume(track.volume),
                        })
                        .collect()
                });
                let request = TrackRequest {
                    audio,
                    subtitle: selection.subtitle,
                };
                Self::Command(selection.id, Command::Tracks(request))
            }
            _ => return Err(format!("unknown media command {command}")),
        })
    }
}
