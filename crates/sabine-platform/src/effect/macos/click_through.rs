use std::{cell::RefCell, ptr::NonNull, rc::Rc};

use block2::RcBlock;
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSEvent, NSEventMask, NSView, NSWindow};

use crate::WindowRegionRect;

/// Lets clicks outside the input region reach whatever is beneath the
/// window. AppKit only offers that for a whole window, so the window ignores
/// the mouse while the pointer is outside the region and takes it back as
/// soon as the pointer returns. Watching the pointer while the window ignores
/// it needs a global monitor too, since the moves then go to other apps.
pub(super) struct ClickThrough {
    window: Retained<NSWindow>,
    region: Rc<RefCell<Vec<WindowRegionRect>>>,
    monitors: Vec<Retained<AnyObject>>,
}

impl ClickThrough {
    pub(super) fn new(view: Retained<NSView>) -> Option<Self> {
        let window = view.window()?;
        let region = Rc::<RefCell<Vec<WindowRegionRect>>>::default();
        let follow = {
            let window = window.clone();
            let region = Rc::clone(&region);
            move || pass_through_outside(&window, &view, &region.borrow())
        };
        let mask = NSEventMask::MouseMoved
            | NSEventMask::LeftMouseDragged
            | NSEventMask::RightMouseDragged;
        let local_follow = follow.clone();
        let local = RcBlock::new(move |event: NonNull<NSEvent>| {
            local_follow();
            event.as_ptr()
        });
        let global = RcBlock::new(move |_: NonNull<NSEvent>| follow());
        let monitors = unsafe {
            [
                NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local),
                NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global),
            ]
        }
        .into_iter()
        .flatten()
        .collect();
        Some(Self {
            window,
            region,
            monitors,
        })
    }

    pub(super) fn set_region(&self, region: Vec<WindowRegionRect>) {
        *self.region.borrow_mut() = region;
    }
}

impl Drop for ClickThrough {
    fn drop(&mut self) {
        for monitor in &self.monitors {
            unsafe { NSEvent::removeMonitor(monitor) };
        }
        self.window.setIgnoresMouseEvents(false);
    }
}

fn pass_through_outside(window: &NSWindow, view: &NSView, region: &[WindowRegionRect]) {
    let point = view.convertPoint_fromView(
        window.convertPointFromScreen(NSEvent::mouseLocation()),
        None,
    );
    let y = if view.isFlipped() {
        point.y
    } else {
        view.bounds().size.height - point.y
    };
    let inside = region.iter().any(|rect| {
        point.x >= f64::from(rect.x)
            && point.x < f64::from(rect.x + rect.width)
            && y >= f64::from(rect.y)
            && y < f64::from(rect.y + rect.height)
    });
    if window.ignoresMouseEvents() == inside {
        window.setIgnoresMouseEvents(!inside);
    }
}
