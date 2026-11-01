use sha2::{Digest, Sha256};
use std::{
    fs::{File, Metadata},
    io::{self, Read},
    path::Path,
    time::UNIX_EPOCH,
};

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

    pub fn bytes(mut self, bytes: &[u8]) -> Self {
        self.0.update(bytes);
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
        hex(&self.0.finalize()[..8])
    }
}

/// `bytes` random bytes from the operating system, as lowercase hexadecimal.
pub fn random_hex(bytes: usize) -> io::Result<String> {
    let mut random = vec![0_u8; bytes];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    Ok(hex(&random))
}

/// Lowercase hexadecimal, as release manifests and CEF indexes write digests.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The hexadecimal digest of everything `reader` yields.
pub(crate) fn digest_reader<D: Digest>(
    mut reader: impl Read,
    mut on_read: impl FnMut(usize),
) -> io::Result<String> {
    let mut digest = D::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(hex(&digest.finalize()));
        }
        digest.update(&buffer[..read]);
        on_read(read);
    }
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    digest_reader::<Sha256>(File::open(path)?, |_| {})
}
