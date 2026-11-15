// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// A power request with a reason, which `powercfg /requests` lists. Windows
// ends it with the process, so a crashed app keeps nothing awake.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Power::{
    POWER_REQUEST_TYPE, PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired,
    PowerRequestSystemRequired, PowerSetRequest,
};
use windows::Win32::System::SystemServices::POWER_REQUEST_CONTEXT_VERSION;
use windows::Win32::System::Threading::{
    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};
use windows::core::PWSTR;

pub(super) struct Blocker {
    request: HANDLE,
    kinds: &'static [POWER_REQUEST_TYPE],
}

// SAFETY: a power request handle may be set, cleared and closed from any
// thread.
unsafe impl Send for Blocker {}
unsafe impl Sync for Blocker {}

impl Blocker {
    pub(super) async fn new(display: bool, reason: &str) -> Result<Self, String> {
        let kinds: &'static [POWER_REQUEST_TYPE] = if display {
            &[PowerRequestSystemRequired, PowerRequestDisplayRequired]
        } else {
            &[PowerRequestSystemRequired]
        };
        let request = request(reason, kinds).map_err(|error| error.to_string())?;
        Ok(Self { request, kinds })
    }
}

fn request(reason: &str, kinds: &[POWER_REQUEST_TYPE]) -> windows::core::Result<HANDLE> {
    let mut reason = reason
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let context = REASON_CONTEXT {
        Version: POWER_REQUEST_CONTEXT_VERSION,
        Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
        Reason: REASON_CONTEXT_0 {
            SimpleReasonString: PWSTR(reason.as_mut_ptr()),
        },
    };
    unsafe {
        let request = PowerCreateRequest(&context)?;
        for kind in kinds {
            if let Err(error) = PowerSetRequest(request, *kind) {
                let _ = CloseHandle(request);
                return Err(error);
            }
        }
        Ok(request)
    }
}

impl Drop for Blocker {
    fn drop(&mut self) {
        unsafe {
            for kind in self.kinds {
                let _ = PowerClearRequest(self.request, *kind);
            }
            let _ = CloseHandle(self.request);
        }
    }
}
