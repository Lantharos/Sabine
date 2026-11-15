//! An IOKit power assertion with a name, which `pmset -g assertions`
//! lists. macOS releases it with the process.

use objc2_foundation::{NSString, ns_string};

const ASSERTION_LEVEL_ON: u32 = 255;
const SUCCESS: i32 = 0;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: &NSString,
        level: u32,
        name: &NSString,
        assertion: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(assertion: u32) -> i32;
}

pub(super) struct Blocker {
    assertion: u32,
}

impl Blocker {
    pub(super) async fn new(display: bool, reason: &str) -> Result<Self, String> {
        let kind = if display {
            ns_string!("PreventUserIdleDisplaySleep")
        } else {
            ns_string!("PreventUserIdleSystemSleep")
        };
        let mut assertion = 0;
        let result = unsafe {
            IOPMAssertionCreateWithName(
                kind,
                ASSERTION_LEVEL_ON,
                &NSString::from_str(reason),
                &mut assertion,
            )
        };
        if result != SUCCESS {
            return Err(format!("IOKit refused the power assertion ({result:#x})"));
        }
        Ok(Self { assertion })
    }
}

impl Drop for Blocker {
    fn drop(&mut self) {
        unsafe { IOPMAssertionRelease(self.assertion) };
    }
}
