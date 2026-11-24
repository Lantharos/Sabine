use sabine_bridge::BridgeHandlers;
use serde::Deserialize;

use super::process::WindowId;
use crate::window::config::SabineWindowConfig;
use crate::{SabineError, SabineLifecyclePolicy, SabineProcess, SabineResult};

/// A window one of the app's pages asked to open on one of the app's own
/// pages, with `sabine.window.open`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageWindow {
    url: String,
    title: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    min_width: Option<u32>,
    min_height: Option<u32>,
    resizable: Option<bool>,
    always_on_top: Option<bool>,
    overlay: Option<PageOverlay>,
}

/// `overlay` in `sabine.window.open`: `true`, or where the overlay sits.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PageOverlay {
    On(bool),
    At(crate::SabineOverlay),
}

impl PageWindow {
    /// The app's first window as an ordinary window of the page's size:
    /// shown, in the taskbar, and closing when it is closed.
    fn config(&self, first: &SabineWindowConfig) -> SabineWindowConfig {
        let mut config = first.clone();
        config.title = self.title.clone().unwrap_or(config.title);
        config.width = self.width.unwrap_or(config.width);
        config.height = self.height.unwrap_or(config.height);
        config.min_width = self.min_width.unwrap_or(config.min_width);
        config.min_height = self.min_height.unwrap_or(config.min_height);
        config.resizable = self.resizable.unwrap_or(true);
        config.always_on_top = self.always_on_top.unwrap_or(false);
        config.max_size = None;
        config.maximized = false;
        config.remember_size = false;
        config.visible = true;
        config.active = true;
        config.hide_on_blur = false;
        config.hide_on_close = false;
        config.skip_taskbar = false;
        config.lifecycle = SabineLifecyclePolicy::browser_tab();
        config.overlay = None;
        match self.overlay {
            Some(PageOverlay::On(true)) => config.show_as_overlay(crate::SabineOverlay::default()),
            Some(PageOverlay::At(placement)) => config.show_as_overlay(placement),
            _ => {}
        }
        config
    }
}

impl SabineProcess {
    pub(super) fn open_page_window(&mut self, page: PageWindow) -> SabineResult<WindowId> {
        let url = app_page_url(&self.windows.url, &page.url)?;
        let config = page.config(&self.windows.primary);
        config.validate()?;
        crate::osr::launch::attach_open_window(self, &config, BridgeHandlers::default(), &url)
    }
}

/// `page` resolved against the app's entry, which it must stay on the origin
/// of, so pages open only the app's own pages.
fn app_page_url(entry: &str, page: &str) -> SabineResult<String> {
    let invalid = |message: String| SabineError::CreationFailed { message };
    let entry = url::Url::parse(entry).map_err(|error| invalid(error.to_string()))?;
    let url = entry
        .join(page)
        .map_err(|error| invalid(error.to_string()))?;
    let same_app = url.scheme() == entry.scheme()
        && url.host_str() == entry.host_str()
        && url.port_or_known_default() == entry.port_or_known_default();
    if !same_app {
        return Err(invalid(format!(
            "{url} is not one of the app's pages, which are all a page can open in a window"
        )));
    }
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::app_page_url;

    #[test]
    fn pages_open_the_apps_own_pages() {
        assert_eq!(
            app_page_url("sabine://app/index.html", "settings.html#general").unwrap(),
            "sabine://app/settings.html#general"
        );
        assert_eq!(
            app_page_url("http://localhost:5173/", "/settings").unwrap(),
            "http://localhost:5173/settings"
        );
    }

    #[test]
    fn pages_cannot_open_other_sites() {
        for page in [
            "https://example.com/",
            "//example.com/",
            "http://localhost:5174/",
            "file:///etc/passwd",
        ] {
            assert!(
                app_page_url("http://localhost:5173/", page).is_err(),
                "{page}"
            );
        }
        assert!(app_page_url("sabine://app/index.html", "sabine://file/etc/passwd").is_err());
    }
}
