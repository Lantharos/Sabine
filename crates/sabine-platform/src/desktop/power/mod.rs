#[cfg(target_os = "linux")]
#[path = "linux/mod.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos/mod.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows/mod.rs"]
mod platform;

use std::sync::{Arc, Mutex};

/// A change to the machine's power or the person's session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerEvent {
    /// The system is about to sleep.
    Suspending,
    /// The system woke from sleep.
    Resumed,
    /// The screen locked.
    Locked,
    /// The screen unlocked.
    Unlocked,
    /// The machine switched between its battery and mains power, or its
    /// battery's charge changed.
    Source(PowerSource),
}

/// Where the machine draws its power from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerSource {
    pub on_battery: bool,
    /// The battery's charge in percent, on machines with a battery.
    pub battery_level: Option<u8>,
}

/// The power and session state the desktop last reported. `source` is
/// `None` until the desktop has told it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PowerState {
    pub source: Option<PowerSource>,
    pub locked: bool,
}

pub type PowerEvents = Arc<dyn Fn(PowerEvent) + Send + Sync>;

/// Reads the current [`PowerState`] of a [`PowerWatcher`] from any thread.
#[derive(Clone, Default)]
pub struct PowerStatus(Arc<Mutex<PowerState>>);

impl PowerStatus {
    pub fn get(&self) -> PowerState {
        *self.lock()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, PowerState> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// What the platform watchers report, passed on only when it changes the
/// state, so a repeated report reaches nobody.
#[derive(Clone)]
struct Reporter {
    status: PowerStatus,
    events: PowerEvents,
}

impl Reporter {
    fn suspending(&self) {
        (self.events)(PowerEvent::Suspending);
    }

    fn resumed(&self) {
        (self.events)(PowerEvent::Resumed);
    }

    fn locked(&self, locked: bool) {
        let changed = std::mem::replace(&mut self.status.lock().locked, locked) != locked;
        if changed {
            (self.events)(if locked {
                PowerEvent::Locked
            } else {
                PowerEvent::Unlocked
            });
        }
    }

    fn source(&self, source: PowerSource) {
        let changed = self.status.lock().source.replace(source) != Some(source);
        if changed {
            (self.events)(PowerEvent::Source(source));
        }
    }
}

/// Hears the system suspend and resume, the screen lock and unlock, and
/// the power source change, from the moment it starts until it is dropped.
/// Linux asks logind and UPower, Windows its power and session
/// notifications, and macOS the workspace's notifications and IOKit's power
/// sources.
pub struct PowerWatcher {
    status: PowerStatus,
    _platform: platform::Watcher,
}

impl PowerWatcher {
    pub fn start(events: PowerEvents) -> Self {
        let status = PowerStatus::default();
        let platform = platform::Watcher::start(Reporter {
            status: status.clone(),
            events,
        });
        Self {
            status,
            _platform: platform,
        }
    }

    pub fn status(&self) -> PowerStatus {
        self.status.clone()
    }
}
