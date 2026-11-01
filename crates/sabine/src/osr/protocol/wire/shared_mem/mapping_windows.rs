use std::io;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Memory::{
    FILE_MAP_READ, MEMORY_BASIC_INFORMATION, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
    UnmapViewOfFile, VirtualQuery,
};

pub(super) struct SharedMapping {
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    len: usize,
}

// Safety: the view is read-only in this process and unmapped only on drop.
unsafe impl Send for SharedMapping {}
unsafe impl Sync for SharedMapping {}

impl SharedMapping {
    /// Maps the file mapping the browser host opened in this process. The
    /// view keeps the mapping alive, so its handle is closed right away.
    pub(super) fn map(handle: u64, max_len: usize) -> io::Result<Self> {
        let handle = HANDLE(handle as *mut std::ffi::c_void);
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_READ, 0, 0, 0) };
        unsafe {
            let _ = CloseHandle(handle);
        }
        if view.Value.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut region = MEMORY_BASIC_INFORMATION::default();
        let queried = unsafe {
            VirtualQuery(
                Some(view.Value.cast_const()),
                &mut region,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        let mapping = Self {
            view,
            len: region.RegionSize,
        };
        if queried == 0 || mapping.len == 0 || mapping.len > max_len {
            return Err(super::outside_limit());
        }
        Ok(mapping)
    }

    pub(super) fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.view.Value.cast(), self.len) }
    }
}

impl Drop for SharedMapping {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
        }
    }
}
