use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSColor, NSColorSpace, NSSystemColorsDidChangeNotification};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};
use winit::window::{Theme, Window};

use super::{Appearance, channel};

/// The window's theme with the system accent color, which it keeps current
/// and wakes the window about whenever macOS changes its system colors.
pub struct AppearanceWatcher {
    accent: Arc<Mutex<Option<[u8; 3]>>>,
    observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl AppearanceWatcher {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let accent = Arc::new(Mutex::new(accent_color()));
        let watched = Arc::clone(&accent);
        let changed = RcBlock::new(move |_: NonNull<NSNotification>| {
            *watched.lock().unwrap_or_else(|error| error.into_inner()) = accent_color();
            wake();
        });
        let observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSSystemColorsDidChangeNotification),
                None,
                None,
                &changed,
            )
        };
        Self { accent, observer }
    }

    pub fn appearance(&self, window: &dyn Window) -> Appearance {
        Appearance {
            dark: window.theme() == Some(Theme::Dark),
            accent: *self
                .accent
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        }
    }
}

impl Drop for AppearanceWatcher {
    fn drop(&mut self) {
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(self.observer.as_ref()) };
    }
}

fn accent_color() -> Option<[u8; 3]> {
    let accent =
        NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some([
        channel(accent.redComponent()),
        channel(accent.greenComponent()),
        channel(accent.blueComponent()),
    ])
}
