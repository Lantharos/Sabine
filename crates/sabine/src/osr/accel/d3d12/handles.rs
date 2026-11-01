// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The browser host duplicates each owned texture's NT handle into this process
// once. The handle value is only meaningful here, so it must stay open until the
// host retires the resource; closing it early lets Windows reuse the value for an
// unrelated object that a later import would silently open.

use std::collections::HashMap;
use std::sync::Arc;

use windows::Win32::Foundation::{CloseHandle, HANDLE};

#[derive(Debug)]
pub(crate) struct SharedHandle(HANDLE);

// Safety: an NT handle is a process-wide kernel object reference.
unsafe impl Send for SharedHandle {}
unsafe impl Sync for SharedHandle {}

impl SharedHandle {
    /// Takes ownership of a handle the browser host opened in this process.
    fn adopt(value: u64) -> Self {
        Self(HANDLE(value as *mut std::ffi::c_void))
    }

    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for SharedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Handles announced by the browser host, keyed by resource id.
#[derive(Default)]
pub(crate) struct SharedHandles(HashMap<u64, Arc<SharedHandle>>);

impl SharedHandles {
    pub(crate) fn resolve(
        &mut self,
        resource_id: u64,
        announced: u64,
    ) -> Option<Arc<SharedHandle>> {
        if announced != 0 {
            self.0
                .insert(resource_id, Arc::new(SharedHandle::adopt(announced)));
        }
        self.0.get(&resource_id).cloned()
    }

    pub(crate) fn retire(&mut self, resource_ids: impl IntoIterator<Item = u64>) {
        for resource_id in resource_ids {
            self.0.remove(&resource_id);
        }
    }
}
