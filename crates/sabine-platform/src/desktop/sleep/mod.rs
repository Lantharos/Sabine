#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

/// Keeps the machine from sleeping while it lives, with a reason the
/// desktop may show people. Linux asks the XDG desktop portal, or logind
/// where the portal can't, Windows makes a power request and macOS an IOKit
/// power assertion.
pub struct SleepBlocker {
    _platform: platform::Blocker,
}

impl SleepBlocker {
    /// Keeps the system awake; the screen may still turn off.
    pub async fn system(reason: &str) -> Result<Self, String> {
        Self::new(false, reason).await
    }

    /// Keeps the screen on, and so the system awake.
    pub async fn display(reason: &str) -> Result<Self, String> {
        Self::new(true, reason).await
    }

    async fn new(display: bool, reason: &str) -> Result<Self, String> {
        Ok(Self {
            _platform: platform::Blocker::new(display, reason).await?,
        })
    }
}
