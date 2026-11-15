//! IOKit's power sources, read again each time IOKit says they changed.
//! Its snapshots are Core Foundation objects, which Foundation reads as the
//! dictionaries, arrays and strings they bridge to.

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_foundation::{CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopDefaultMode};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, ns_string};

use super::super::{PowerSource, Reporter};

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPSNotificationCreateRunLoopSource(
        callback: extern "C" fn(*mut c_void),
        context: *mut c_void,
    ) -> *mut CFRunLoopSource;
    fn IOPSCopyPowerSourcesInfo() -> *mut c_void;
    fn IOPSGetProvidingPowerSourceType(snapshot: *const c_void) -> *const NSString;
    fn IOPSCopyPowerSourcesList(snapshot: *const c_void) -> *mut NSArray<AnyObject>;
    fn IOPSGetPowerSourceDescription(
        snapshot: *const c_void,
        source: *const AnyObject,
    ) -> *const NSDictionary<NSString, AnyObject>;
}

/// IOKit holds the reporter as the source's context until the source is
/// invalidated, which happens before the reporter goes.
pub(super) struct Sources {
    source: Option<CFRetained<CFRunLoopSource>>,
    reporter: *mut Reporter,
}

impl Sources {
    pub(super) fn watch(reporter: Reporter) -> Self {
        report(&reporter);
        let reporter = Box::into_raw(Box::new(reporter));
        let source =
            NonNull::new(unsafe { IOPSNotificationCreateRunLoopSource(changed, reporter.cast()) })
                .map(|source| unsafe { CFRetained::from_raw(source) });
        if let (Some(source), Some(main)) = (&source, CFRunLoop::main()) {
            main.add_source(Some(source), unsafe { kCFRunLoopDefaultMode });
        }
        Self { source, reporter }
    }
}

impl Drop for Sources {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            source.invalidate();
        }
        drop(unsafe { Box::from_raw(self.reporter) });
    }
}

extern "C" fn changed(reporter: *mut c_void) {
    report(unsafe { &*reporter.cast::<Reporter>() });
}

fn report(reporter: &Reporter) {
    if let Some(source) = read() {
        reporter.source(source);
    }
}

fn read() -> Option<PowerSource> {
    let snapshot = unsafe { IOPSCopyPowerSourcesInfo() };
    let snapshot = unsafe { Retained::from_raw(snapshot.cast::<AnyObject>()) }?;
    let snapshot = Retained::as_ptr(&snapshot).cast::<c_void>();
    let providing = unsafe { IOPSGetProvidingPowerSourceType(snapshot).as_ref() }?;
    let sources = unsafe { Retained::from_raw(IOPSCopyPowerSourcesList(snapshot)) }?;
    let battery_level = sources.iter().find_map(|source| {
        let description =
            unsafe { IOPSGetPowerSourceDescription(snapshot, Retained::as_ptr(&source)).as_ref() }?;
        let text = |key: &NSString| description.objectForKey(key)?.downcast::<NSString>().ok();
        let number = |key: &NSString| {
            description
                .objectForKey(key)?
                .downcast::<NSNumber>()
                .ok()
                .map(|number| number.as_f64())
        };
        if text(ns_string!("Type"))?.to_string() != "InternalBattery" {
            return None;
        }
        let capacity = number(ns_string!("Max Capacity")).filter(|capacity| *capacity > 0.0)?;
        let level = number(ns_string!("Current Capacity"))? / capacity * 100.0;
        Some(level.round().clamp(0.0, 100.0) as u8)
    });
    Some(PowerSource {
        on_battery: providing.to_string() == "Battery Power",
        battery_level,
    })
}
