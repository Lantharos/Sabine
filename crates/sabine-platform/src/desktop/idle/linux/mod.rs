//! The compositor tells when the person goes idle through
//! `ext-idle-notify-v1`, and GNOME's through its idle monitor on the session
//! bus. Neither tells how long the person has been away.

mod mutter;
mod wayland;

use std::time::Duration;

use super::IdleEvents;

/// A watch on whichever of the two the desktop offers, which ends once
/// dropped.
pub(super) enum Watch {
    Wayland { _watch: wayland::Watch },
    Mutter { _watch: mutter::Watch },
}

impl Watch {
    pub(super) async fn start(threshold: Duration, events: IdleEvents) -> Result<Self, String> {
        match wayland::Watch::start(threshold, events.clone()) {
            Ok(watch) => Ok(Self::Wayland { _watch: watch }),
            Err(wayland) => mutter::Watch::start(threshold, events)
                .await
                .map(|watch| Self::Mutter { _watch: watch })
                .map_err(|mutter| format!("{wayland}; {mutter}")),
        }
    }
}
