#[cfg(unix)]
mod mapping_unix;
#[cfg(windows)]
mod mapping_windows;

use std::{collections::HashMap, io, sync::Arc};

#[cfg(unix)]
use mapping_unix::SharedMapping;
#[cfg(windows)]
use mapping_windows::SharedMapping;

use crate::osr::control::ControlWriter;

/// How the browser host hands a paint slot's memory over the first time: a
/// file descriptor on Linux and macOS, a file mapping handle it opened in
/// this process on Windows.
#[cfg(unix)]
pub(super) type AnnouncedMapping = i32;
#[cfg(windows)]
pub(super) type AnnouncedMapping = u64;

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
        announced: Option<AnnouncedMapping>,
        max_len: usize,
    ) -> io::Result<Arc<PaintLease>> {
        if let Some(announced) = announced {
            let mapping = Arc::new(SharedMapping::map(announced, max_len)?);
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

fn outside_limit() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "shared OSR paint buffer size is outside the protocol limit",
    )
}
