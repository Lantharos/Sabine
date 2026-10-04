//! macOS handles its own shortcuts, such as Cmd+Tab, Cmd+Space and Cmd+`,
//! before any app sees them. A session event tap sees them first: while the
//! window is the key window of the active app, it takes every key that has
//! Command, Control or Option held and posts it to the app itself, so the
//! page receives it like any other key. Event taps need the Accessibility
//! permission.

use std::{ffi::c_void, ptr::NonNull};

use objc2::{rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSApplication, NSEvent, NSView, NSWindow};
use objc2_core_foundation::{
    CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes,
};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};
use objc2_foundation::{MainThreadMarker, NSDictionary, NSNumber, NSString};

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
}

const SHORTCUT_MODIFIERS: CGEventFlags = CGEventFlags(
    CGEventFlags::MaskCommand.0 | CGEventFlags::MaskControl.0 | CGEventFlags::MaskAlternate.0,
);

struct Tap {
    window: Retained<NSWindow>,
    application: Retained<NSApplication>,
    port: Option<CFRetained<CFMachPort>>,
}

pub(super) struct MacInhibitor {
    tap: Box<Tap>,
    source: CFRetained<CFRunLoopSource>,
}

impl MacInhibitor {
    pub(super) fn new(view: &NSView) -> Result<Self, String> {
        let main_thread =
            MainThreadMarker::new().ok_or("Shortcuts can only be inhibited on the main thread")?;
        if !accessibility_trusted() {
            return Err(
                "Allow this app in System Settings › Privacy & Security › Accessibility so it can receive the system's keyboard shortcuts"
                    .to_string(),
            );
        }
        let window = view.window().ok_or("The view has no window")?;
        let mut tap = Box::new(Tap {
            window,
            application: NSApplication::sharedApplication(main_thread),
            port: None,
        });
        let mask = (1 << CGEventType::KeyDown.0) | (1 << CGEventType::KeyUp.0);
        let port = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                mask,
                Some(divert_shortcut),
                (tap.as_mut() as *mut Tap).cast(),
            )
        }
        .ok_or("macOS refused the keyboard event tap")?;
        let source = CFMachPort::new_run_loop_source(None, Some(&port), 0)
            .ok_or("Could not listen to the keyboard event tap")?;
        let run_loop = CFRunLoop::main().ok_or("The app has no main run loop")?;
        run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        tap.port = Some(port);
        Ok(Self { tap, source })
    }
}

impl Drop for MacInhibitor {
    fn drop(&mut self) {
        if let Some(port) = self.tap.port.take() {
            CGEvent::tap_enable(&port, false);
            port.invalidate();
        }
        self.source.invalidate();
    }
}

unsafe extern "C-unwind" fn divert_shortcut(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    tap: *mut c_void,
) -> *mut CGEvent {
    let tap = unsafe { &*tap.cast::<Tap>() };
    if matches!(
        kind,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    ) {
        if let Some(port) = &tap.port {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }
    let flags = CGEvent::flags(Some(unsafe { event.as_ref() }));
    let ours = tap.application.isActive() && tap.window.isKeyWindow();
    if !ours || flags.0 & SHORTCUT_MODIFIERS.0 == 0 {
        return event.as_ptr();
    }
    match NSEvent::eventWithCGEvent(unsafe { event.as_ref() }) {
        Some(key) => {
            tap.application.postEvent_atStart(&key, false);
            std::ptr::null_mut()
        }
        None => event.as_ptr(),
    }
}

/// Asks macOS whether the app may tap keyboard events, and lets it show the
/// prompt that leads to the Accessibility settings when it may not.
fn accessibility_trusted() -> bool {
    let options = NSDictionary::<NSString, AnyObject>::from_slices(
        &[&*NSString::from_str("AXTrustedCheckOptionPrompt")],
        &[NSNumber::new_bool(true).as_ref()],
    );
    unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options).cast()) != 0 }
}
