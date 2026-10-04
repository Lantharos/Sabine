use sha2::{Digest, Sha256};
use std::{fs::Metadata, io, path::Path, time::UNIX_EPOCH};

/// A short identifier for cache directories that every Sabine build computes
/// identically, independent of the Rust toolchain that compiled it.
#[derive(Default)]
pub struct Fingerprint(Sha256);

impl Fingerprint {
    pub fn path(mut self, path: &Path) -> Self {
        self.0.update(path.as_os_str().as_encoded_bytes());
        self.0.update([0]);
        self
    }

    pub fn file(mut self, metadata: &Metadata) -> io::Result<Self> {
        let modified = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        self.0.update(metadata.len().to_le_bytes());
        self.0.update(modified.as_secs().to_le_bytes());
        self.0.update(modified.subsec_nanos().to_le_bytes());
        Ok(self)
    }

    pub fn finish(self) -> String {
        self.0.finalize()[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}
