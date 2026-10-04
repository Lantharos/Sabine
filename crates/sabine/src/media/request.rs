use sabine_bridge::media as commands;
use serde::Deserialize;
use serde_json::Value;

use super::command::{Command, TrackRequest};
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
                Self::Create(serde_json::from_value(params).map_err(invalid)?)
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
                let rate: Number = serde_json::from_value(params).map_err(invalid)?;
                if !(rate.value > 0.0 && rate.value <= 16.0) {
                    return Err("playback rate must be above 0 and at most 16".to_string());
                }
                Self::Command(rate.id, Command::Rate(rate.value))
            }
            commands::SET_VOLUME_COMMAND => {
                let volume: Number = serde_json::from_value(params).map_err(invalid)?;
                Self::Command(volume.id, Command::Volume(volume.value))
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
                let choice = |name: &str| {
                    params
                        .get(name)
                        .map(|value| value.as_str().map(str::to_string))
                };
                let request = TrackRequest {
                    audio: choice("audio"),
                    subtitle: choice("subtitle"),
                };
                Self::Command(id(params)?, Command::Tracks(request))
            }
            _ => return Err(format!("unknown media command {command}")),
        })
    }
}
