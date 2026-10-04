//! Native media surfaces: GStreamer playback presented in a Wayland
//! subsurface beneath the page, for media Chromium cannot decode.

mod geometry;
mod gst;
mod player;
mod present;
mod request;
mod source;
mod wayland;

use std::{collections::BTreeMap, thread::JoinHandle};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use serde_json::{Value, json};
use winit::window::Window;

pub(crate) use geometry::{MediaHole, Rect, Viewport};
pub(crate) use source::SourcePolicy;

use geometry::{Layout, PageRect};
use gst::Handle;
pub(crate) use player::Events;
use player::{Command, Player, PlayerOptions};
use request::Request;
use wayland::{MediaWayland, Placement};

/// Whether a window with this configuration can show media surfaces: they
/// show through the page, so the window must be transparent.
pub(crate) fn supported(config: &crate::window::config::SabineWindowConfig) -> bool {
    config.transparent
}

pub(crate) struct MediaHost {
    transparent: bool,
    wayland: Option<MediaWayland>,
    display: Option<Handle>,
    parent: Option<Handle>,
    viewport: Option<Viewport>,
    occluded: bool,
    sessions: BTreeMap<u64, Session>,
    stopping: Vec<JoinHandle<()>>,
    next_id: u64,
    changed: bool,
}

struct Session {
    player: Player,
    placement: Option<Placement>,
    rect: Option<PageRect>,
    layout: Option<Layout>,
}

impl MediaHost {
    pub(crate) fn new(transparent: bool) -> Self {
        Self {
            transparent,
            wayland: None,
            display: None,
            parent: None,
            viewport: None,
            occluded: false,
            sessions: BTreeMap::new(),
            stopping: Vec::new(),
            next_id: 1,
            changed: false,
        }
    }

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
                self.stop([session]);
                self.changed = true;
                self.flush();
                return Ok(Value::Null);
            }
            Request::SetRect(id, rect) => {
                self.session(id)?.rect = rect;
                self.relayout(id);
                self.flush();
                return Ok(Value::Null);
            }
            Request::Play(id) => (id, Command::Play),
            Request::Pause(id) => (id, Command::Pause),
            Request::Seek { id, time, fast } => (id, Command::Seek { time, fast }),
            Request::Rate(id, rate) => (id, Command::Rate(rate)),
            Request::Volume(id, volume) => (id, Command::Volume(volume)),
            Request::Muted(id, muted) => (id, Command::Muted(muted)),
            Request::Loop(id, looping) => (id, Command::Loop(looping)),
            Request::Tracks(id, tracks) => (id, Command::Tracks(tracks)),
        };
        self.session(id)?.player.send(command);
        Ok(Value::Null)
    }

    fn create(
        &mut self,
        options: PlayerOptions,
        events: impl FnOnce(u64) -> Events,
    ) -> Result<u64, String> {
        if !self.transparent {
            return Err("native media needs a transparent window".to_string());
        }
        let display = self.display.ok_or("native media needs a Wayland window")?;
        let target = match self.parent {
            Some(parent) => Some(self.create_surface(display, parent)?),
            None => None,
        };
        let id = self.next_id;
        self.next_id += 1;
        let (placement, target) = target.unzip();
        let player = Player::spawn(options, target, display, events(id));
        if self.occluded {
            player.send(Command::Occluded(true));
        }
        self.sessions.insert(
            id,
            Session {
                player,
                placement,
                rect: None,
                layout: None,
            },
        );
        Ok(id)
    }

    fn create_surface(
        &mut self,
        display: Handle,
        parent: Handle,
    ) -> Result<(Placement, wayland::Target), String> {
        let wayland = match &mut self.wayland {
            Some(wayland) => wayland,
            None => self
                .wayland
                .insert(unsafe { MediaWayland::connect(display.0) }?),
        };
        unsafe { wayland.create_surface(parent.0) }
    }

    /// Places media surfaces beneath a newly created window.
    pub(crate) fn attach(&mut self, window: &dyn Window, viewport: Viewport) -> Result<(), String> {
        let (Ok(RawDisplayHandle::Wayland(display)), Ok(RawWindowHandle::Wayland(surface))) = (
            window.display_handle().map(|handle| handle.as_raw()),
            window.window_handle().map(|handle| handle.as_raw()),
        ) else {
            return Ok(());
        };
        let display = Handle(display.display.as_ptr());
        let parent = Handle(surface.surface.as_ptr());
        self.display = Some(display);
        self.parent = Some(parent);
        self.viewport = Some(viewport);
        let ids = self.sessions.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let (placement, target) = self.create_surface(display, parent)?;
            if let Some(session) = self.sessions.get_mut(&id) {
                session.placement = Some(placement);
                session.layout = None;
                session.player.send(Command::Target(Some(target)));
            }
            self.relayout(id);
        }
        self.flush();
        Ok(())
    }

    /// Removes media surfaces before their window goes away.
    pub(crate) fn detach(&mut self) {
        self.parent = None;
        for session in self.sessions.values_mut() {
            session.placement = None;
            session.layout = None;
            session.player.send(Command::Target(None));
        }
        self.changed = true;
        self.flush();
    }

    /// Stops every player, as when the page that created them goes away.
    pub(crate) fn clear(&mut self) {
        if !self.sessions.is_empty() {
            let sessions = std::mem::take(&mut self.sessions);
            self.stop(sessions.into_values());
            self.changed = true;
            self.flush();
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
        self.flush();
    }

    pub(crate) fn set_occluded(&mut self, occluded: bool) {
        if self.occluded != occluded {
            self.occluded = occluded;
            for session in self.sessions.values() {
                session.player.send(Command::Occluded(occluded));
            }
        }
    }

    /// Window areas where the page shows media, for the compositor to leave clear.
    pub(crate) fn holes(&self) -> impl Iterator<Item = MediaHole> + '_ {
        self.sessions
            .values()
            .filter_map(|session| session.layout.map(|layout| layout.hole))
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
            .placement
            .as_ref()
            .and(session.rect)
            .zip(viewport)
            .and_then(|(rect, viewport)| rect.layout(viewport));
        if layout == session.layout {
            return;
        }
        if let (Some(layout), Some(placement)) = (layout, &session.placement) {
            placement
                .subsurface
                .set_position(layout.hole.surface.x, layout.hole.surface.y);
        }
        session.layout = layout;
        session
            .player
            .send(Command::Frame(layout.map(|layout| layout.frame)));
        self.changed = true;
    }

    fn flush(&mut self) {
        if let Some(wayland) = &mut self.wayland {
            wayland.flush();
        }
    }

    fn stop(&mut self, sessions: impl IntoIterator<Item = Session>) {
        self.stopping.retain(|thread| !thread.is_finished());
        self.stopping
            .extend(sessions.into_iter().map(|session| session.player.stop()));
    }
}

impl Drop for MediaHost {
    /// Waits for every player to release its surfaces while the window's
    /// Wayland connection is still open.
    fn drop(&mut self) {
        self.clear();
        for thread in self.stopping.drain(..) {
            let _ = thread.join();
        }
        present::terminate();
    }
}
