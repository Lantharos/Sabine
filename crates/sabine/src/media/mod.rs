//! Native media surfaces: platform playback presented on a surface stacked
//! beneath the page, for media Chromium cannot decode.

mod backend;
mod command;
mod geometry;
mod playing;
mod request;
mod source;
mod tracks;

use std::collections::BTreeMap;

use serde_json::{Value, json};
use winit::window::Window;

pub(crate) use command::Events;
pub(crate) use geometry::{MediaHole, Rect, Viewport};
pub(crate) use source::SourcePolicy;

use backend::{Backend, Native, Player};
use command::PlayerOptions;
use geometry::{Layout, PageRect};
use playing::PlayingSessions;
use request::Request;

/// Whether a window with this configuration can show media surfaces: they
/// show through the page, so the window must be transparent.
pub(crate) fn supported(config: &crate::window::config::SabineWindowConfig) -> bool {
    config.transparent
}

#[derive(Default)]
pub(crate) struct MediaHost {
    backend: Native,
    attached: bool,
    viewport: Option<Viewport>,
    occluded: bool,
    sessions: BTreeMap<u64, Session>,
    next_id: u64,
    changed: bool,
    playing: PlayingSessions,
}

struct Session {
    player: <Native as Backend>::Player,
    mounted: bool,
    rect: Option<PageRect>,
    layout: Option<Layout>,
}

impl MediaHost {
    /// Handles a `sabine.media.*` command from the page.
    pub(crate) fn handle(
        &mut self,
        command: &str,
        params: Value,
        policy: &SourcePolicy,
        events: impl FnOnce(u64) -> Events,
    ) -> Result<Value, String> {
        let (id, command) = match Request::parse(command, params)? {
            Request::Create(create) => {
                let options = PlayerOptions {
                    uri: source::resolve(&create.src, policy)?,
                    autoplay: create.autoplay,
                    looping: create.looping,
                    volume: create.volume,
                    muted: create.muted,
                    rate: create.rate,
                };
                return self.create(options, events).map(|id| json!({ "id": id }));
            }
            Request::Destroy(id) => {
                let session = self
                    .sessions
                    .remove(&id)
                    .ok_or_else(|| format!("media {id} does not exist"))?;
                self.backend.stop(session.player);
                self.playing.set(id, false);
                self.changed = true;
                self.backend.flush();
                return Ok(Value::Null);
            }
            Request::SetRect(id, rect) => {
                self.session(id)?.rect = rect;
                self.relayout(id);
                self.backend.flush();
                return Ok(Value::Null);
            }
            Request::Command(id, command) => (id, command),
        };
        self.session(id)?.player.send(command);
        Ok(Value::Null)
    }

    fn create(
        &mut self,
        options: PlayerOptions,
        events: impl FnOnce(u64) -> Events,
    ) -> Result<u64, String> {
        self.next_id += 1;
        let id = self.next_id;
        let mut player = self
            .backend
            .spawn(options, self.playing.track(id, events(id)))?;
        if self.attached
            && let Err(error) = self.backend.mount(&mut player)
        {
            self.backend.stop(player);
            return Err(error);
        }
        if self.occluded {
            player.set_occluded(true);
        }
        self.sessions.insert(
            id,
            Session {
                player,
                mounted: self.attached,
                rect: None,
                layout: None,
            },
        );
        Ok(id)
    }

    /// Places media surfaces beneath a newly created window.
    pub(crate) fn attach(&mut self, window: &dyn Window, viewport: Viewport) -> Result<(), String> {
        self.backend.attach(window)?;
        self.attached = true;
        self.viewport = Some(viewport);
        let ids = self.sessions.keys().copied().collect::<Vec<_>>();
        for id in ids {
            if let Some(session) = self.sessions.get_mut(&id) {
                self.backend.mount(&mut session.player)?;
                session.mounted = true;
                session.layout = None;
            }
            self.relayout(id);
        }
        self.backend.flush();
        Ok(())
    }

    /// Removes media surfaces before their window goes away.
    pub(crate) fn detach(&mut self) {
        for session in self.sessions.values_mut() {
            session.player.unmount();
            session.mounted = false;
            session.layout = None;
        }
        self.backend.detach();
        self.attached = false;
        self.changed = true;
        self.backend.flush();
    }

    /// Stops every player, as when the page that created them goes away.
    pub(crate) fn clear(&mut self) {
        if !self.sessions.is_empty() {
            for session in std::mem::take(&mut self.sessions).into_values() {
                self.backend.stop(session.player);
            }
            self.playing.clear();
            self.changed = true;
            self.backend.flush();
        }
    }

    pub(crate) fn set_viewport(&mut self, viewport: Viewport) {
        if self.viewport == Some(viewport) {
            return;
        }
        self.viewport = Some(viewport);
        let ids = self.sessions.keys().copied().collect::<Vec<_>>();
        for id in ids {
            self.relayout(id);
        }
        self.backend.flush();
    }

    pub(crate) fn set_occluded(&mut self, occluded: bool) {
        if self.occluded != occluded {
            self.occluded = occluded;
            for session in self.sessions.values_mut() {
                session.player.set_occluded(occluded);
            }
        }
    }

    /// Window areas where the page shows media, for the compositor to leave clear.
    pub(crate) fn holes(&self) -> impl Iterator<Item = MediaHole> + '_ {
        self.sessions
            .values()
            .filter_map(|session| session.layout.map(|layout| layout.hole))
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playing.any()
    }

    /// Whether the holes changed since the last call.
    pub(crate) fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    fn session(&mut self, id: u64) -> Result<&mut Session, String> {
        self.sessions
            .get_mut(&id)
            .ok_or_else(|| format!("media {id} does not exist"))
    }

    fn relayout(&mut self, id: u64) {
        let viewport = self.viewport;
        let Some(session) = self.sessions.get_mut(&id) else {
            return;
        };
        let layout = session
            .rect
            .filter(|_| session.mounted)
            .zip(viewport)
            .and_then(|(rect, viewport)| rect.layout(viewport));
        if layout == session.layout {
            return;
        }
        session.layout = layout;
        session.player.place(layout);
        self.changed = true;
    }
}

impl Drop for MediaHost {
    fn drop(&mut self) {
        self.clear();
    }
}
