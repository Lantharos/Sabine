//! The badge and progress bar on the app's Dock tile. The badge is the
//! tile's own; while there is progress, the tile shows a view that draws the
//! app's icon with a bar across its foot.

use std::cell::{Cell, RefCell};

use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{NSApplication, NSBezierPath, NSColor, NSView};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use sabine_platform::{AppBadge, AppProgress};

use super::{UiQueue, main_queue_thread};

thread_local! {
    static PROGRESS: RefCell<Option<Retained<ProgressView>>> = const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabineDockProgress"]
    #[ivars = Cell<AppProgress>]
    struct ProgressView;

    impl ProgressView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            let bounds = self.bounds();
            if let Some(icon) = NSApplication::sharedApplication(self.mtm()).applicationIconImage() {
                icon.drawInRect(bounds);
            }
            let (color, filled) = match self.ivars().get() {
                AppProgress::Hidden => return,
                AppProgress::Indeterminate => (NSColor::systemGrayColor(), 1.0),
                AppProgress::Normal(value) => (NSColor::controlAccentColor(), value),
                AppProgress::Paused(value) => (NSColor::systemYellowColor(), value),
                AppProgress::Error(value) => (NSColor::systemRedColor(), value),
            };
            let track = NSRect::new(
                NSPoint::new(bounds.size.width * 0.1, bounds.size.height * 0.08),
                NSSize::new(bounds.size.width * 0.8, bounds.size.height * 0.1),
            );
            let bar = NSRect::new(
                track.origin,
                NSSize::new(track.size.width * filled.clamp(0.0, 1.0), track.size.height),
            );
            let radius = track.size.height / 2.0;
            NSColor::colorWithWhite_alpha(0.0, 0.45).setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(track, radius, radius).fill();
            color.setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bar, radius, radius).fill();
        }
    }
);

pub(in crate::desktop) fn set_badge(badge: Option<AppBadge>) {
    UiQueue.run(move || {
        let label = badge
            .as_ref()
            .and_then(AppBadge::label)
            .map(|label| NSString::from_str(&label));
        NSApplication::sharedApplication(main_queue_thread())
            .dockTile()
            .setBadgeLabel(label.as_deref());
    });
}

pub(in crate::desktop) fn set_progress(progress: AppProgress) {
    UiQueue.run(move || {
        let main_thread = main_queue_thread();
        let tile = NSApplication::sharedApplication(main_thread).dockTile();
        let changed = PROGRESS.with(|view| {
            let mut view = view.borrow_mut();
            if view
                .as_ref()
                .is_some_and(|view| view.ivars().get() == progress)
            {
                return false;
            }
            if progress == AppProgress::Hidden {
                if view.take().is_some() {
                    tile.setContentView(None);
                }
            } else {
                let view = view.get_or_insert_with(|| {
                    let frame = NSRect::new(NSPoint::new(0.0, 0.0), tile.size());
                    let view: Retained<ProgressView> = unsafe {
                        msg_send![
                            super(ProgressView::alloc(main_thread).set_ivars(Cell::new(progress))),
                            initWithFrame: frame
                        ]
                    };
                    tile.setContentView(Some(&**view));
                    view
                });
                view.ivars().set(progress);
            }
            true
        });
        if changed {
            tile.display();
        }
    });
}
