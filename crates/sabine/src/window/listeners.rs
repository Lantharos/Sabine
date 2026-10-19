use std::{fmt, sync::Arc};

use sabine_platform::Appearance;

use super::visibility::{VISIBILITY_LINE, VisibilityListener, WindowVisibility};
use crate::{SabineProcessHandle, WindowId};

pub(crate) const APPEARANCE_LINE: &str = "SABINE_SYSTEM_APPEARANCE";

/// The app's listeners for what a window host reports about its window.
#[derive(Clone, Debug, Default)]
pub(crate) struct WindowListeners {
    pub(crate) visibility: Option<VisibilityListener>,
    pub(crate) appearance: Option<AppearanceListener>,
}

impl WindowListeners {
    /// Delivers a report line from the window host. Returns false for lines
    /// that are not reports.
    pub(crate) fn deliver(&self, window: WindowId, line: &str) -> bool {
        if line.starts_with(VISIBILITY_LINE) {
            if let Some(listener) = &self.visibility {
                listener.deliver(window, line);
            }
            return true;
        }
        if line.starts_with(APPEARANCE_LINE) {
            if let Some(listener) = &self.appearance {
                listener.deliver(window, line);
            }
            return true;
        }
        false
    }

    /// Reports a window whose host has exited as hidden.
    pub(crate) fn closed(&self, window: WindowId) {
        if let Some(listener) = &self.visibility {
            listener.report(WindowVisibility {
                window,
                visible: false,
                suspended: false,
            });
        }
    }
}

#[derive(Clone)]
pub(crate) struct AppearanceListener(Arc<dyn Fn(WindowId, Appearance) + Send + Sync>);

impl AppearanceListener {
    pub(crate) fn new(listener: impl Fn(WindowId, Appearance) + Send + Sync + 'static) -> Self {
        Self(Arc::new(listener))
    }

    fn deliver(&self, window: WindowId, line: &str) {
        let mut parts = line.split('\t').skip(1);
        let (Some(dark), Some(accent)) = (parts.next(), parts.next()) else {
            return;
        };
        (self.0)(
            window,
            Appearance {
                dark: dark == "1",
                accent: parse_hex(accent),
            },
        );
    }
}

/// The line a window host writes when the appearance changes.
pub(crate) fn appearance_line(appearance: &Appearance) -> String {
    format!(
        "{APPEARANCE_LINE}\t{}\t{}",
        u8::from(appearance.dark),
        appearance.accent_hex().unwrap_or_default()
    )
}

fn parse_hex(value: &str) -> Option<[u8; 3]> {
    let value = value.strip_prefix('#')?;
    let channel = |index: usize| u8::from_str_radix(value.get(index..index + 2)?, 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

impl fmt::Debug for AppearanceListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AppearanceListener")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_lines_carry_the_accent_color() {
        let appearance = Appearance {
            dark: true,
            accent: Some([0x35, 0x84, 0xe4]),
        };
        let delivered = Arc::new(std::sync::Mutex::new(None));
        let received = Arc::clone(&delivered);
        let listeners = WindowListeners {
            appearance: Some(AppearanceListener::new(move |_, appearance| {
                *received.lock().unwrap() = Some(appearance);
            })),
            ..WindowListeners::default()
        };
        assert!(listeners.deliver(7, &appearance_line(&appearance)));
        assert_eq!(*delivered.lock().unwrap(), Some(appearance));
    }
}

/// The app's listener for its process launching, for app code that presents
/// or opens windows later on.
#[derive(Clone)]
pub(crate) struct LaunchListener(Arc<dyn Fn(SabineProcessHandle) + Send + Sync>);

impl LaunchListener {
    pub(crate) fn new(listener: impl Fn(SabineProcessHandle) + Send + Sync + 'static) -> Self {
        Self(Arc::new(listener))
    }

    pub(crate) fn deliver(&self, handle: SabineProcessHandle) {
        (self.0)(handle);
    }
}

impl fmt::Debug for LaunchListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LaunchListener")
    }
}
