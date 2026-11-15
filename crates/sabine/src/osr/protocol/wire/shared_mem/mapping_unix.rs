use std::{
    fs::File,
    io,
    os::fd::{AsRawFd, OwnedFd},
    ptr,
};

/// The seals that keep the browser host from resizing a paint slot under
/// this process's mapping, which would fault reads past its new end.
#[cfg(target_os = "linux")]
const REQUIRED_SEALS: i32 = libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL;

pub(super) struct SharedMapping {
    ptr: *mut u8,
    len: usize,
}

// Safety: the mapping is read-only in this process and unmapped only on drop.
unsafe impl Send for SharedMapping {}
unsafe impl Sync for SharedMapping {}

impl SharedMapping {
    /// Maps a paint slot. On Linux the slot must be sealed against resizing;
    /// a macOS shared memory object keeps the size it was first given.
    pub(super) fn map(fd: OwnedFd, max_len: usize) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GET_SEALS) } & REQUIRED_SEALS
            != REQUIRED_SEALS
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "shared OSR paint buffer is not sealed against resizing",
            ));
        }
        let file = File::from(fd);
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
