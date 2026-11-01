use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

/// The processor a 64-bit Windows application executable is built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::bundle) enum WindowsArchitecture {
    X64,
    Arm64,
}

impl WindowsArchitecture {
    pub(in crate::bundle) fn of(binary: &Path) -> Result<Self, String> {
        let header = read_header(binary)
            .map_err(|error| format!("could not inspect Windows executable: {error}"))?;
        if &header[..4] != b"PE\0\0" {
            return Err("Windows bundles require a PE executable".into());
        }
        let characteristics = u16::from_le_bytes([header[22], header[23]]);
        if characteristics & 0x2002 != 0x0002 || header[24..26] != [0x0b, 0x02] {
            return Err("Windows bundles require a 64-bit PE application executable".into());
        }
        match u16::from_le_bytes([header[4], header[5]]) {
            0x8664 => Ok(Self::X64),
            0xaa64 => Ok(Self::Arm64),
            _ => Err("Windows bundles require an x86_64 or ARM64 executable".into()),
        }
    }

    /// The architecture as WiX, CMake and Visual Studio name it.
    pub(in crate::bundle) fn msvc_name(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "ARM64",
        }
    }

    /// The architecture as Rust names it.
    pub(in crate::bundle) fn rust_name(self) -> &'static str {
        match self {
            Self::X64 => "x86_64",
            Self::Arm64 => "aarch64",
        }
    }
}

/// The PE signature, file header and optional header magic of `binary`.
fn read_header(binary: &Path) -> std::io::Result<[u8; 26]> {
    let mut file = fs::File::open(binary)?;
    let mut dos = [0_u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "missing DOS header",
        ));
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().expect("four bytes"));
    file.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut header = [0_u8; 26];
    file.read_exact(&mut header)?;
    Ok(header)
}
