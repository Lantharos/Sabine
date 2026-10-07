use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, PoisonError},
};

use serde_json::Value;

use super::Events;

/// The players currently playing, learned from the state events every backend
/// reports, so a window with media playing is never frozen.
#[derive(Clone, Default)]
pub(super) struct PlayingSessions(Arc<Mutex<BTreeSet<u64>>>);

impl PlayingSessions {
    pub(super) fn track(&self, id: u64, events: Events) -> Events {
        let playing = self.clone();
        Arc::new(move |payload: Value| {
            if payload["type"] == "state" {
                match payload["state"].as_str() {
                    Some("playing") => playing.set(id, true),
                    Some("paused" | "ended") => playing.set(id, false),
                    _ => {}
                }
            }
            events(payload);
        })
    }

    pub(super) fn set(&self, id: u64, playing: bool) {
        let mut sessions = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if playing {
            sessions.insert(id);
        } else {
            sessions.remove(&id);
        }
    }

    pub(super) fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    pub(super) fn any(&self) -> bool {
        !self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    }
}
