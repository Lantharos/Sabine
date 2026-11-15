#[cfg(target_os = "linux")]
#[path = "linux/mod.rs"]
mod platform;
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[path = "timer.rs"]
mod platform;

use std::sync::Arc;
use std::time::Duration;

/// Whether the person uses the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleState {
    Active,
    Idle,
}

impl IdleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Idle => "idle",
        }
    }
}

pub type IdleEvents = Arc<dyn Fn(IdleState) + Send + Sync>;

/// Hears the person leave the machine and come back, until it is dropped.
/// A watch starts out active.
pub struct IdleWatch {
    _platform: platform::Watch,
}

impl IdleWatch {
    /// Reports [`IdleState::Idle`] once nobody used the keyboard, pointer or
    /// touch for `threshold`, and [`IdleState::Active`] as soon as someone
    /// does again. Fails on a Linux desktop whose compositor offers neither
    /// `ext-idle-notify-v1` nor GNOME's idle monitor.
    pub async fn start(threshold: Duration, events: IdleEvents) -> Result<Self, String> {
        Ok(Self {
            _platform: platform::Watch::start(threshold, events).await?,
        })
    }
}
