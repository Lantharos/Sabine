use std::ptr::NonNull;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSApplicationDidChangeScreenParametersNotification, NSScreen};
use objc2_core_foundation::CGRect;
use objc2_core_graphics::CGDisplayBounds;
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSNumber, NSObjectProtocol, ns_string,
};
use winit::monitor::MonitorHandle;

use super::DisplayRect;

/// Wakes the window whenever macOS connects, disconnects or rearranges a
/// display, or changes its resolution or the space the dock and menu bar
/// take.
pub struct DisplayWatcher {
    observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl DisplayWatcher {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let changed = RcBlock::new(move |_: NonNull<NSNotification>| wake());
        let observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
                None,
                &changed,
            )
        };
        Self { observer }
    }
}

impl Drop for DisplayWatcher {
    fn drop(&mut self) {
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(self.observer.as_ref()) };
    }
}

/// The display's bounds as Core Graphics places it, with its top left corner
/// at the top left of the main display, and the part of it the dock and menu
/// bar leave, which AppKit gives from the bottom left instead.
pub(super) fn rects(monitor: &MonitorHandle) -> Option<(DisplayRect, Option<DisplayRect>)> {
    let id = u32::try_from(monitor.native_id()).ok()?;
    let scale = monitor.scale_factor();
    let bounds = CGDisplayBounds(id);
    let screen = screen(id)?;
    let frame = screen.frame();
    let visible = screen.visibleFrame();
    let top = (frame.origin.y + frame.size.height) - (visible.origin.y + visible.size.height);
    let work_area = CGRect::new(
        objc2_core_foundation::CGPoint::new(
            bounds.origin.x + visible.origin.x - frame.origin.x,
            bounds.origin.y + top,
        ),
        visible.size,
    );
    Some((physical(bounds, scale), Some(physical(work_area, scale))))
}

fn screen(id: u32) -> Option<Retained<NSScreen>> {
    let main_thread = MainThreadMarker::new()?;
    NSScreen::screens(main_thread).into_iter().find(|screen| {
        screen
            .deviceDescription()
            .objectForKey(ns_string!("NSScreenNumber"))
            .and_then(|number| number.downcast::<NSNumber>().ok())
            .is_some_and(|number| number.unsignedIntValue() == id)
    })
}

fn physical(rect: CGRect, scale: f64) -> DisplayRect {
    DisplayRect {
        x: (rect.origin.x * scale).round() as i32,
        y: (rect.origin.y * scale).round() as i32,
        width: (rect.size.width * scale).round() as u32,
        height: (rect.size.height * scale).round() as u32,
    }
}
