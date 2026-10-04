//! GStreamer playback presented in a Wayland subsurface beneath the window.

mod gst;
mod player;
mod present;
mod wayland;

use std::thread::JoinHandle;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::window::Window;

use super::{Backend, Player};
use crate::media::command::{Command, Events, PlayerOptions};
use crate::media::geometry::Layout;
use gst::Handle;
use player::{Control, Worker};
use wayland::{MediaWayland, Placement};

#[derive(Default)]
pub(in crate::media) struct GStreamer {
    wayland: Option<MediaWayland>,
    display: Option<Handle>,
    parent: Option<Handle>,
    stopping: Vec<JoinHandle<()>>,
}

pub(in crate::media) struct GstPlayer {
    worker: Worker,
    placement: Option<Placement>,
}

impl Backend for GStreamer {
    type Player = GstPlayer;

    fn attach(&mut self, window: &dyn Window) -> Result<(), String> {
        let (Ok(RawDisplayHandle::Wayland(display)), Ok(RawWindowHandle::Wayland(surface))) = (
            window.display_handle().map(|handle| handle.as_raw()),
            window.window_handle().map(|handle| handle.as_raw()),
        ) else {
            return Err("native media needs a Wayland window".to_string());
        };
        self.display = Some(Handle(display.display.as_ptr()));
        self.parent = Some(Handle(surface.surface.as_ptr()));
        Ok(())
    }

    fn detach(&mut self) {
        self.parent = None;
    }

    fn spawn(&mut self, options: PlayerOptions, events: Events) -> Result<GstPlayer, String> {
        let display = self.display.ok_or("native media needs a Wayland window")?;
        Ok(GstPlayer {
            worker: Worker::spawn(options, display, events),
            placement: None,
        })
    }

    fn mount(&mut self, player: &mut GstPlayer) -> Result<(), String> {
        let (Some(display), Some(parent)) = (self.display, self.parent) else {
            return Err("native media needs a Wayland window".to_string());
        };
        let wayland = match &mut self.wayland {
            Some(wayland) => wayland,
            None => self
                .wayland
                .insert(unsafe { MediaWayland::connect(display.0) }?),
        };
        let (placement, target) = unsafe { wayland.create_surface(parent.0) }?;
        player.placement = Some(placement);
        player.worker.send(Control::Target(Some(target)));
        Ok(())
    }

    fn stop(&mut self, player: GstPlayer) {
        self.stopping.retain(|thread| !thread.is_finished());
        self.stopping.push(player.worker.stop());
    }

    fn flush(&mut self) {
        if let Some(wayland) = &mut self.wayland {
            wayland.flush();
        }
    }
}

impl Drop for GStreamer {
    /// Waits for every player to release its surfaces while the window's
    /// Wayland connection is still open.
    fn drop(&mut self) {
        for thread in self.stopping.drain(..) {
            let _ = thread.join();
        }
        present::terminate();
    }
}

impl Player for GstPlayer {
    fn send(&mut self, command: Command) {
        self.worker.send(Control::Page(command));
    }

    fn place(&mut self, layout: Option<Layout>) {
        if let (Some(layout), Some(placement)) = (layout, &self.placement) {
            placement
                .subsurface
                .set_position(layout.hole.surface.x, layout.hole.surface.y);
        }
        self.worker
            .send(Control::Frame(layout.map(|layout| layout.frame)));
    }

    fn unmount(&mut self) {
        self.placement = None;
        self.worker.send(Control::Target(None));
    }

    fn set_occluded(&mut self, occluded: bool) {
        self.worker.send(Control::Occluded(occluded));
    }
}
