use sabine_platform::WindowRegionRect;

use super::chrome::{configured_control_at, configured_region_at, titlebar_control_at};
use crate::SabineWindowControlRegion;
use crate::osr::host::types::TitlebarControl;

/// Where clicks reach the window's controls and move the window, rather than
/// the page. App regions are in app pixels, scaled by the page's zoom; page
/// regions are in page pixels below the titlebar.
pub(super) struct ChromeHits<'a> {
    pub controls: &'a [SabineWindowControlRegion],
    pub drag: &'a [WindowRegionRect],
    pub drag_exclusions: &'a [WindowRegionRect],
    pub page_drag: &'a [WindowRegionRect],
    pub page_drag_exclusions: &'a [WindowRegionRect],
    pub titlebar_height: f32,
    pub zoom: f32,
}

impl<'a> ChromeHits<'a> {
    const NONE: ChromeHits<'static> = ChromeHits {
        controls: &[],
        drag: &[],
        drag_exclusions: &[],
        page_drag: &[],
        page_drag_exclusions: &[],
        titlebar_height: 0.0,
        zoom: 1.0,
    };

    /// A full-screen window has no controls or drag areas, so the page gets
    /// every click, even where a control sits while the window floats.
    pub fn unless_fullscreen(self, fullscreen: bool) -> Self {
        if fullscreen { Self::NONE } else { self }
    }

    pub fn control_at(&self, width: f32, x: f32, y: f32) -> Option<TitlebarControl> {
        let zoom = self.zoom;
        configured_control_at(self.controls, width / zoom, x / zoom, y / zoom)
            .or_else(|| titlebar_control_at(width, self.titlebar_height, x, y))
    }

    pub fn is_drag_region(&self, width: f32, x: f32, y: f32) -> bool {
        let page_y = y - self.titlebar_height;
        let page_excluded =
            page_y >= 0.0 && configured_region_at(self.page_drag_exclusions, width, x, page_y);
        let zoom = self.zoom;
        let (app_width, app_x, app_y) = (width / zoom, x / zoom, y / zoom);
        if configured_region_at(self.drag_exclusions, app_width, app_x, app_y) || page_excluded {
            return false;
        }
        if !self.drag.is_empty() || !self.page_drag.is_empty() {
            let page_draggable =
                page_y >= 0.0 && configured_region_at(self.page_drag, width, x, page_y);
            return configured_region_at(self.drag, app_width, app_x, app_y) || page_draggable;
        }
        self.titlebar_height > 0.0 && y <= self.titlebar_height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SabineWindowControlAction;

    const WIDTH: f32 = 1200.0;

    fn close() -> Vec<SabineWindowControlRegion> {
        vec![SabineWindowControlRegion {
            action: SabineWindowControlAction::Close,
            rect: WindowRegionRect::new(-46, 0, 46, 38),
        }]
    }

    fn titlebar() -> Vec<WindowRegionRect> {
        vec![WindowRegionRect::new(0, 0, i32::MAX, 38)]
    }

    #[test]
    fn a_full_screen_window_leaves_its_controls_and_drag_areas_to_the_page() {
        let (controls, drag) = (close(), titlebar());
        let hits = |fullscreen| {
            ChromeHits {
                controls: &controls,
                drag: &drag,
                ..ChromeHits::NONE
            }
            .unless_fullscreen(fullscreen)
        };
        assert_eq!(
            hits(false).control_at(WIDTH, WIDTH - 20.0, 10.0),
            Some(TitlebarControl::Close)
        );
        assert!(hits(false).is_drag_region(WIDTH, 300.0, 10.0));

        assert_eq!(hits(true).control_at(WIDTH, WIDTH - 20.0, 10.0), None);
        assert!(!hits(true).is_drag_region(WIDTH, 300.0, 10.0));
    }
}
