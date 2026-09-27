use std::{
    collections::HashMap,
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
    ptr,
    sync::Arc,
};

use crate::osr::control::ControlWriter;

struct SharedMapping {
    ptr: *mut u8,
    len: usize,
}

// Safety: the mapping is read-only in this process and unmapped only on drop.
unsafe impl Send for SharedMapping {}
unsafe impl Sync for SharedMapping {}

impl SharedMapping {
    fn map(fd: i32, max_len: usize) -> io::Result<Self> {
        let file = unsafe { File::from_raw_fd(fd) };
        let len = usize::try_from(file.metadata()?.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "shared OSR buffer does not fit this platform",
            )
        })?;
        if len == 0 || len > max_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "shared OSR paint buffer size is outside the protocol limit",
            ));
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

    fn as_slice(&self) -> &[u8] {
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

/// Pixels in a producer-owned paint slot. Dropping the last lease hands the
/// slot back to Chromium for its next paint.
pub(crate) struct PaintLease {
    mapping: Arc<SharedMapping>,
    slot: u32,
    generation: u32,
    control: Arc<ControlWriter>,
}

impl PaintLease {
    pub(crate) fn as_slice(&self) -> &[u8] {
        self.mapping.as_slice()
    }
}

impl std::fmt::Debug for PaintLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PaintLease")
            .field("slot", &self.slot)
            .field("generation", &self.generation)
            .finish()
    }
}

impl Drop for PaintLease {
    fn drop(&mut self) {
        let _ = self.control.send(format!(
            "paint_release\t{}\t{}\n",
            self.slot, self.generation
        ));
    }
}

pub(crate) struct PaintSlots {
    mappings: HashMap<u32, (u32, Arc<SharedMapping>)>,
    control: Arc<ControlWriter>,
}

impl PaintSlots {
    pub(crate) fn new(control: Arc<ControlWriter>) -> Self {
        Self {
            mappings: HashMap::new(),
            control,
        }
    }

    pub(super) fn lease(
        &mut self,
        slot: u32,
        generation: u32,
        announced_fd: Option<i32>,
        max_len: usize,
    ) -> io::Result<Arc<PaintLease>> {
        if let Some(fd) = announced_fd {
            let mapping = Arc::new(SharedMapping::map(fd, max_len)?);
            self.mappings.insert(slot, (generation, mapping));
        }
        let mapping = match self.mappings.get(&slot) {
            Some((current, mapping)) if *current == generation => Arc::clone(mapping),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "OSR paint references an unknown shared slot",
                ));
            }
        };
        Ok(Arc::new(PaintLease {
            mapping,
            slot,
            generation,
            control: Arc::clone(&self.control),
        }))
    }
}
