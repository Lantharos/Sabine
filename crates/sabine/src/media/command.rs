use std::sync::Arc;

use serde_json::Value;

/// Reports an event about one player to its page.
pub(crate) type Events = Arc<dyn Fn(Value) + Send + Sync>;

/// Emits a `type`-tagged event.
pub(super) fn emit(events: &Events, kind: &str, mut payload: Value) {
    payload["type"] = kind.into();
    events(payload);
}

pub(super) struct PlayerOptions {
    pub(super) uri: String,
    pub(super) autoplay: bool,
    pub(super) looping: bool,
    pub(super) volume: f64,
    pub(super) muted: bool,
    pub(super) rate: f64,
}

/// A playback request from the page.
pub(super) enum Command {
    Play,
    Pause,
    Seek { time: f64, fast: bool },
    Rate(f64),
    Volume(f64),
    Muted(bool),
    Loop(bool),
    Tracks(TrackRequest),
}

/// A track choice from the page: `None` keeps the current choice and
/// `Some(None)` turns the track kind off.
#[derive(Clone, Default)]
pub(super) struct TrackRequest {
    pub(super) audio: Option<Option<String>>,
    pub(super) subtitle: Option<Option<String>>,
}
