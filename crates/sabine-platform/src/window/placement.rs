use winit::window::Window;

use crate::linux::kestrel::{
    kestrel_window_manager_v1::KestrelWindowManagerV1,
    kestrel_window_state_v1::KestrelWindowStateV1,
};
use crate::linux::wayland::Client;

const MANAGER: &str = "kestrel_window_manager_v1";
const RESIZES_FROM_OUTSIDE_VERSION: u32 = 2;

/// Whether the compositor lets people resize the window from just outside its
/// edges, as Kestrel does from version 2 of `kestrel_window_v1`, so the window
/// needs no resize area of its own over its content.
pub fn resizes_from_outside(window: &dyn Window) -> bool {
    Client::connect(window)
        .ok()
        .and_then(|client| client.version_of(MANAGER))
        .is_some_and(|version| version >= RESIZES_FROM_OUTSIDE_VERSION)
}

/// Keeps a Wayland window out of the taskbar and window switcher, or above
/// other windows, through `kestrel_window_v1`. Dropping it returns the window
/// to its default placement.
pub struct WindowPlacement {
    manager: KestrelWindowManagerV1,
    state: KestrelWindowStateV1,
    client: Client,
}

impl WindowPlacement {
    pub fn new(
        window: &dyn Window,
        skip_window_list: bool,
        keep_above: bool,
    ) -> Result<Self, String> {
        let client = Client::connect(window)?;
        let toplevel = client.toplevel(window)?;
        let manager = client
            .globals
            .bind::<KestrelWindowManagerV1, _, _>(&client.handle(), 1..=1, ())
            .map_err(
                |_| "This desktop cannot keep windows out of its taskbar or above other windows",
            )?;
        let state = manager.get_window_state(&toplevel, &client.handle(), ());
        if skip_window_list {
            state.set_skip_window_list();
        }
        if keep_above {
            state.set_keep_above();
        }
        client.flush();
        Ok(Self {
            manager,
            state,
            client,
        })
    }
}

impl Drop for WindowPlacement {
    fn drop(&mut self) {
        self.state.destroy();
        self.manager.destroy();
        self.client.flush();
    }
}
