use std::{
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
    ptr,
};

pub(super) struct SharedMapping {
    ptr: *mut u8,
    len: usize,
}

// Safety: the mapping is read-only in this process and unmapped only on drop.
unsafe impl Send for SharedMapping {}
unsafe impl Sync for SharedMapping {}

impl SharedMapping {
    pub(super) fn map(fd: i32, max_len: usize) -> io::Result<Self> {
        let file = unsafe { File::from_raw_fd(fd) };
        let len = usize::try_from(file.metadata()?.len()).map_err(|_| super::outside_limit())?;
        if len == 0 || len > max_len {
            return Err(super::outside_limit());
        }
        let ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            ptr: ptr.cast(),
            len,
        })
    }

    pub(super) fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Drop for SharedMapping {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.cast(), self.len);
        }
    }
}
