use objc2_app_kit::{NSView, NSWindowButton};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Space between the traffic lights and the page's first control.
const TRAFFIC_LIGHT_GAP: f64 = 12.0;

/// The top-left corner the macOS traffic lights cover in a window whose page
/// extends under its titlebar, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlsOverlay {
    pub left: f64,
    pub height: f64,
}

pub fn controls_overlay(window: &dyn Window) -> Option<ControlsOverlay> {
    let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let window = view.window()?;
    let zoom = window.standardWindowButton(NSWindowButton::ZoomButton)?;
    let zoom = zoom.frame();
    Some(ControlsOverlay {
        left: zoom.origin.x + zoom.size.width + TRAFFIC_LIGHT_GAP,
        height: window.frame().size.height - window.contentLayoutRect().size.height,
    })
}
