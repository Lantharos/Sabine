mod gstreamer;

use winit::window::Window;

pub(super) use gstreamer::GStreamer as Native;

use super::command::{Command, Events, PlayerOptions};
use super::geometry::Layout;

/// A platform's way of playing media on surfaces stacked beneath a window's
/// page, through the page's transparent holes.
pub(super) trait Backend: Default {
    type Player: Player;

    /// Prepares to stack surfaces beneath `window`.
    fn attach(&mut self, window: &dyn Window) -> Result<(), String>;

    /// Forgets the window after every player was unmounted.
    fn detach(&mut self);

    fn spawn(&mut self, options: PlayerOptions, events: Events) -> Result<Self::Player, String>;

    /// Gives a player a surface beneath the attached window.
    fn mount(&mut self, player: &mut Self::Player) -> Result<(), String>;

    fn stop(&mut self, player: Self::Player);

    /// Sends surface changes made since the last flush to the compositor.
    fn flush(&mut self);
}

pub(super) trait Player {
    fn send(&mut self, command: Command);

    /// Moves the player's surface, or hides it for `None`.
    fn place(&mut self, layout: Option<Layout>);

    /// Takes the player's surface away before its window goes away.
    fn unmount(&mut self);

    /// Tells a player whether its window is hidden. Only presenters that
    /// draw every frame themselves need to know; the system compositor
    /// handles the others.
    fn set_occluded(&mut self, _occluded: bool) {}
}
