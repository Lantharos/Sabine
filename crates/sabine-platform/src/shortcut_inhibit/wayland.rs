use wayland_client::protocol::wl_seat::WlSeat;
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
    zwp_keyboard_shortcuts_inhibitor_v1::ZwpKeyboardShortcutsInhibitorV1,
};
use winit::window::Window;

use crate::linux::wayland::Client;

/// A `zwp_keyboard_shortcuts_inhibitor_v1` on the window's surface. The
/// compositor applies it whenever the surface has keyboard focus.
pub(super) struct WaylandInhibitor {
    manager: ZwpKeyboardShortcutsInhibitManagerV1,
    inhibitor: ZwpKeyboardShortcutsInhibitorV1,
    client: Client,
}

impl WaylandInhibitor {
    pub(super) fn new(window: &dyn Window) -> Result<Self, String> {
        let client = Client::connect(window)?;
        let surface = client.surface(window)?;
        let handle = client.handle();
        let manager = client
            .globals
            .bind::<ZwpKeyboardShortcutsInhibitManagerV1, _, _>(&handle, 1..=1, ())
            .map_err(|_| "This desktop does not let apps inhibit its shortcuts")?;
        let Ok(seat) = client.globals.bind::<WlSeat, _, _>(&handle, 1..=1, ()) else {
            manager.destroy();
            return Err("This desktop has no keyboard seat".to_string());
        };
        let inhibitor = manager.inhibit_shortcuts(&surface, &seat, &handle, ());
        client.flush();
        Ok(Self {
            manager,
            inhibitor,
            client,
        })
    }
}

impl Drop for WaylandInhibitor {
    fn drop(&mut self) {
        self.inhibitor.destroy();
        self.manager.destroy();
        self.client.flush();
    }
}
