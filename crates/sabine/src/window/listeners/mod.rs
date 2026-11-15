mod closing;
mod downloads;
mod menu;
mod permissions;
mod visibility;

use std::{fmt, sync::Arc};

use sabine_platform::{Appearance, Display, PlatformEvent};

use crate::desktop::EventQueue;
use crate::desktop::open_urls::OpenUrls;
use crate::osr::protocol::{NOTIFICATION_EVENT_LINE, notification_event_from_json};
use crate::{SabineProcessHandle, WindowId};
pub(crate) use closing::{CLOSE_REQUESTED_LINE, CloseRequestListener, ClosedListener};
pub(crate) use downloads::{DOWNLOAD_LINE, DownloadListeners};
pub use downloads::{Download, DownloadControl, DownloadDecision, DownloadState};
pub(crate) use menu::{MENU_ITEM_LINE, MenuItemListener, menu_item_line};
pub(crate) use permissions::{PERMISSION_REQUEST_LINE, PermissionListener};
pub use permissions::{PermissionDecision, PermissionRequest};
pub use visibility::WindowVisibility;
pub(crate) use visibility::{VISIBILITY_LINE, VisibilityListener};

pub(crate) const APPEARANCE_LINE: &str = "SABINE_SYSTEM_APPEARANCE";
/// The displays connected to the computer, as JSON, whenever a window sees
/// them change.
pub(crate) const DISPLAYS_LINE: &str = "SABINE_DISPLAYS";
/// The window's page began loading a document.
pub(crate) const PAGE_STARTED_LINE: &str = "SABINE_PAGE_STARTED";
/// The window's page lost its document without leaving it, as when Sabine
/// restarts the browser or the page's renderer crashed.
pub(crate) const PAGE_LOST_LINE: &str = "SABINE_PAGE_LOST";

/// The app's listeners for what a window host reports about its window.
#[derive(Clone, Debug, Default)]
pub(crate) struct WindowListeners {
    pub(crate) visibility: Option<VisibilityListener>,
    pub(crate) appearance: Option<AppearanceListener>,
    pub(crate) close_request: Option<CloseRequestListener>,
    pub(crate) closed: Option<ClosedListener>,
    pub(crate) displays: Option<DisplaysListener>,
    pub(crate) permission: Option<PermissionListener>,
    pub(crate) downloads: DownloadListeners,
    pub(crate) menu_item: Option<MenuItemListener>,
    /// The app's queue of links and documents, which all its windows share.
    pub(crate) open_urls: OpenUrls,
    /// Where the app hears what happened to its windows' notifications.
    pub(crate) desktop_events: Option<EventQueue>,
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
        if line.starts_with(MENU_ITEM_LINE) {
            if let Some(listener) = &self.menu_item {
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
        if let Some(event) = line
            .strip_prefix(NOTIFICATION_EVENT_LINE)
            .and_then(|event| event.strip_prefix('\t'))
        {
            if let (Some(events), Some(event)) =
                (&self.desktop_events, notification_event_from_json(event))
            {
                let _ = events.send(PlatformEvent::Notification { window, event });
            }
            return true;
        }
        match line {
            PAGE_STARTED_LINE => self.open_urls.release(window),
            PAGE_LOST_LINE => self.open_urls.page_lost(window),
            _ => return false,
        }
        true
    }

    /// The window host control that answers what `line` asks of the app: a
    /// page's permission request or download.
    pub(crate) fn answer(&self, window: WindowId, line: &str) -> Option<(&'static str, String)> {
        if line.starts_with(PERMISSION_REQUEST_LINE) {
            return self
                .permission
                .as_ref()?
                .answer(window, line)
                .map(|answer| ("permission", answer));
        }
        if line.starts_with(DOWNLOAD_LINE) {
            return self
                .downloads
                .deliver(window, line)
                .map(|answer| ("download", answer));
        }
        None
    }

    /// Whether the app lets a window close that someone asked to close.
    pub(crate) fn allows_close(&self, window: WindowId) -> bool {
        self.close_request
            .as_ref()
            .is_none_or(|listener| listener.allows(window))
    }

    /// Reports a window whose host has exited as hidden and closed, and
    /// forgets what its pages took from the open queue.
    pub(crate) fn closed(&self, window: WindowId) {
        self.open_urls.release(window);
        if let Some(listener) = &self.visibility {
            listener.report(WindowVisibility {
                window,
                visible: false,
                suspended: false,
            });
        }
        if let Some(listener) = &self.closed {
            listener.deliver(window);
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

type DisplaysFn = dyn Fn(&[Display]) + Send + Sync;

#[derive(Clone)]
pub(crate) struct DisplaysListener(Arc<DisplaysFn>);

impl DisplaysListener {
    pub(crate) fn new(listener: impl Fn(&[Display]) + Send + Sync + 'static) -> Self {
        Self(Arc::new(listener))
    }

    pub(crate) fn deliver(&self, displays: &[Display]) {
        (self.0)(displays);
    }
}

impl fmt::Debug for DisplaysListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DisplaysListener")
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

/// The app's listener for its process ending on its own, to finish or save
/// what it is doing first.
#[derive(Clone)]
pub(crate) struct QuitListener(Arc<dyn Fn() + Send + Sync>);

impl QuitListener {
    pub(crate) fn new(listener: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(listener))
    }

    pub(crate) fn deliver(&self) {
        (self.0)();
    }
}

impl fmt::Debug for QuitListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("QuitListener")
    }
}

type DesktopEventFn = dyn Fn(&SabineProcessHandle, &PlatformEvent) + Send + Sync;

/// The app's listener for tray clicks, global shortcuts, later launches and
/// links, which hears them whether or not a window is open.
#[derive(Clone)]
pub(crate) struct DesktopEventListener(Arc<DesktopEventFn>);

impl DesktopEventListener {
    pub(crate) fn new(
        listener: impl Fn(&SabineProcessHandle, &PlatformEvent) + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(listener))
    }

    pub(crate) fn deliver(&self, handle: &SabineProcessHandle, event: &PlatformEvent) {
        (self.0)(handle, event);
    }
}

impl fmt::Debug for DesktopEventListener {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DesktopEventListener")
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
