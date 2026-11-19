//! CEF's Linux builds ship their libraries with line tables and full symbol
//! tables, about three quarters of the runtime. Chromium never reads them, and
//! crash reports are matched to CEF's builds by the build ID, which lives in a
//! loaded note and stays.

use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

const ELF_MAGIC: [u8; 4] = *b"\x7fELF";
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const SECTION_HEADER_SIZE: usize = 64;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;
const SHT_NOBITS: u32 = 8;
const SHT_REL: u32 = 9;
const SHF_ALLOC: u64 = 0x2;
const SHF_INFO_LINK: u64 = 0x40;
const SECTION_NAMES: &[u8] = b".shstrtab\0";

/// Removes what the loader never maps from every ELF file in the runtime's
/// `Release` folder, as `strip --strip-all` would.
pub(crate) fn strip_runtime_binaries(runtime_dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(runtime_dir.join("Release"))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            strip_elf(&entry.path())?;
        }
    }
    Ok(())
}

fn strip_elf(path: &Path) -> io::Result<()> {
    let mut file = File::options().read(true).write(true).open(path)?;
    let mut header = [0; HEADER_SIZE];
    if file.read_exact(&mut header).is_err() || header[..4] != ELF_MAGIC {
        return Ok(());
    }
    if header[4] != ELFCLASS64 || header[5] != ELFDATA2LSB {
        return Err(unsupported(path, "is not a 64-bit little-endian ELF file"));
    }
    let program_headers = read_table(
        &mut file,
        u64_at(&header, 0x20),
        u16_at(&header, 0x38),
        PROGRAM_HEADER_SIZE,
    )?;
    let sections = read_table(
        &mut file,
        u64_at(&header, 0x28),
        u16_at(&header, 0x3c),
        SECTION_HEADER_SIZE,
    )?
    .as_chunks::<SECTION_HEADER_SIZE>()
    .0
    .iter()
    .map(Section::parse)
    .collect::<Vec<_>>();
    let names = sections
        .get(usize::from(u16_at(&header, 0x3e)))
        .ok_or_else(|| unsupported(path, "has no section name table"))?;
    let names = read_at(&mut file, names.offset, names.size)?;

    let kept = (0..sections.len())
        .filter(|&index| index == 0 || sections[index].flags & SHF_ALLOC != 0)
        .collect::<Vec<_>>();
    if kept.len() + 1 >= sections.len() {
        return Ok(());
    }
    let new_index = |old: u32| {
        kept.iter()
            .position(|&index| index as u32 == old)
            .map(|index| index as u32)
            .ok_or_else(|| unsupported(path, "maps a section that refers to an unmapped one"))
    };

    let mut new_names = vec![0];
    let mut table = Vec::with_capacity((kept.len() + 1) * SECTION_HEADER_SIZE);
    Section::default().write(&mut table);
    for &index in &kept[1..] {
        let mut section = sections[index].clone();
        section.name = append_name(&mut new_names, name_at(&names, section.name));
        if section.link != 0 {
            section.link = new_index(section.link)?;
        }
        if section.info_is_section() {
            section.info = new_index(section.info)?;
        }
        section.write(&mut table);
    }

    let names_offset = mapped_end(&header, &program_headers, &sections, &kept);
    let names_name = append_name(&mut new_names, SECTION_NAMES);
    let names_size = new_names.len() as u64;
    Section {
        name: names_name,
        kind: SHT_STRTAB,
        offset: names_offset,
        size: names_size,
        alignment: 1,
        ..Section::default()
    }
    .write(&mut table);

    let table_offset = (names_offset + names_size).next_multiple_of(8);
    let section_count = kept.len() as u16 + 1;
    file.seek(SeekFrom::Start(names_offset))?;
    file.write_all(&new_names)?;
    file.seek(SeekFrom::Start(table_offset))?;
    file.write_all(&table)?;
    file.set_len(table_offset + table.len() as u64)?;
    file.seek(SeekFrom::Start(0x28))?;
    file.write_all(&table_offset.to_le_bytes())?;
    file.seek(SeekFrom::Start(0x3c))?;
    file.write_all(&section_count.to_le_bytes())?;
    file.write_all(&(section_count - 1).to_le_bytes())?;
    Ok(())
}

/// Where the last byte the loader maps or a kept section holds ends.
fn mapped_end(header: &[u8], program_headers: &[u8], sections: &[Section], kept: &[usize]) -> u64 {
    let program_header_end = u64_at(header, 0x20) + program_headers.len() as u64;
    program_headers
        .as_chunks::<PROGRAM_HEADER_SIZE>()
        .0
        .iter()
        .map(|entry| u64_at(entry, 0x08) + u64_at(entry, 0x20))
        .chain(
            kept.iter()
                .map(|&index| &sections[index])
                .filter(|section| section.kind != SHT_NOBITS)
                .map(|section| section.offset + section.size),
        )
        .fold(program_header_end.max(HEADER_SIZE as u64), u64::max)
}

#[derive(Clone, Default)]
struct Section {
    name: u32,
    kind: u32,
    flags: u64,
    address: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    alignment: u64,
    entry_size: u64,
}

impl Section {
    fn parse(entry: &[u8; SECTION_HEADER_SIZE]) -> Self {
        Self {
            name: u32_at(entry, 0x00),
            kind: u32_at(entry, 0x04),
            flags: u64_at(entry, 0x08),
            address: u64_at(entry, 0x10),
            offset: u64_at(entry, 0x18),
            size: u64_at(entry, 0x20),
            link: u32_at(entry, 0x28),
            info: u32_at(entry, 0x2c),
            alignment: u64_at(entry, 0x30),
            entry_size: u64_at(entry, 0x38),
        }
    }

    fn info_is_section(&self) -> bool {
        self.info != 0
            && (matches!(self.kind, SHT_REL | SHT_RELA) || self.flags & SHF_INFO_LINK != 0)
    }

    fn write(&self, table: &mut Vec<u8>) {
        table.extend_from_slice(&self.name.to_le_bytes());
        table.extend_from_slice(&self.kind.to_le_bytes());
        table.extend_from_slice(&self.flags.to_le_bytes());
        table.extend_from_slice(&self.address.to_le_bytes());
        table.extend_from_slice(&self.offset.to_le_bytes());
        table.extend_from_slice(&self.size.to_le_bytes());
        table.extend_from_slice(&self.link.to_le_bytes());
        table.extend_from_slice(&self.info.to_le_bytes());
        table.extend_from_slice(&self.alignment.to_le_bytes());
        table.extend_from_slice(&self.entry_size.to_le_bytes());
    }
}

fn read_table(file: &mut File, offset: u64, count: u16, entry_size: usize) -> io::Result<Vec<u8>> {
    read_at(file, offset, (usize::from(count) * entry_size) as u64)
}

fn read_at(file: &mut File, offset: u64, size: u64) -> io::Result<Vec<u8>> {
    let mut bytes = vec![0; size as usize];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn name_at(names: &[u8], offset: u32) -> &[u8] {
    let name = names.get(offset as usize..).unwrap_or_default();
    let end = name
        .iter()
        .position(|&byte| byte == 0)
        .map_or(name.len(), |end| end + 1);
    &name[..end]
}

fn append_name(names: &mut Vec<u8>, name: &[u8]) -> u32 {
    let offset = names.len() as u32;
    names.extend_from_slice(name);
    if !name.ends_with(&[0]) {
        names.push(0);
    }
    offset
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn unsupported(path: &Path, problem: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{} {problem}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripped_executable_still_runs() {
        let runtime = tempfile::tempdir().unwrap();
        let binary = runtime.path().join("Release").join("binary");
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
        let original = fs::metadata(&binary).unwrap().len();

        strip_runtime_binaries(runtime.path()).unwrap();
        let stripped = fs::metadata(&binary).unwrap().len();
        strip_runtime_binaries(runtime.path()).unwrap();

        assert!(stripped < original);
        assert_eq!(fs::metadata(&binary).unwrap().len(), stripped);
        let listed = std::process::Command::new(&binary)
            .arg("--list")
            .output()
            .unwrap();
        assert!(listed.status.success());
        assert!(String::from_utf8_lossy(&listed.stdout).contains("stripped_executable_still_runs"));
    }
}
