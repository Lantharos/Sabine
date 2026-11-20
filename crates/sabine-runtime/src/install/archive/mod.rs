mod links;
mod tar;
mod zip;

use std::{
    fs::{self, File},
    io::{self, Read, Seek},
    path::{Component, Path},
};

const MAX_ENTRIES: usize = 100_000;
const MAX_EXPANDED_SIZE: u64 = 16 * 1024 * 1024 * 1024;

/// Extracts a zip archive or a plain, gzip, bzip2 or zstd tar archive into
/// the empty directory `destination`, confining every entry and link to it.
/// Callers validate the payload before activating it.
pub fn extract_archive(archive: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    if fs::read_dir(destination)?.next().is_some() {
        return Err(invalid(
            "archive extraction requires an empty staging directory",
        ));
    }
    let destination = destination.canonicalize()?;
    let mut input = File::open(archive)?;
    let mut signature = [0u8; 4];
    input.read_exact(&mut signature)?;
    input.rewind()?;
    let links = if signature.starts_with(b"PK") {
        zip::extract(input, &destination)?
    } else if signature.starts_with(&[0x1f, 0x8b]) {
        tar::extract(flate2::read::MultiGzDecoder::new(input), &destination)?
    } else if signature.starts_with(b"BZh") {
        tar::extract(bzip2::read::MultiBzDecoder::new(input), &destination)?
    } else if signature == [0x28, 0xb5, 0x2f, 0xfd] {
        tar::extract(zstd::stream::read::Decoder::new(input)?, &destination)?
    } else {
        tar::extract(input, &destination)?
    };
    links::create(&destination, &links)
}

/// What an archive expands to so far, which stays within what any Sabine
/// payload needs.
#[derive(Default)]
struct Expansion {
    entries: usize,
    size: u64,
}

impl Expansion {
    fn add(&mut self, size: u64) -> io::Result<()> {
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(invalid("archive has too many entries"));
        }
        self.size = self
            .size
            .checked_add(size)
            .filter(|total| *total <= MAX_EXPANDED_SIZE)
            .ok_or_else(|| invalid("archive exceeds 16 GiB"))?;
        Ok(())
    }
}

/// Whether an entry name stays below the directory the archive unpacks into.
fn safe_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.to_string_lossy().contains(['\\', ':'])
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn tar_with_links(links: &[(&str, &str)]) -> Vec<u8> {
        let mut builder = ::tar::Builder::new(Vec::new());
        let mut directory = ::tar::Header::new_gnu();
        directory.set_entry_type(::tar::EntryType::Directory);
        directory.set_size(0);
        builder
            .append_data(&mut directory, "cef", io::empty())
            .unwrap();
        for (path, target) in links {
            let mut header = ::tar::Header::new_gnu();
            header.set_entry_type(::tar::EntryType::Symlink);
            header.set_size(0);
            builder.append_link(&mut header, path, target).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn extract_tar(bytes: &[u8]) -> io::Result<()> {
        let scratch = tempfile::tempdir().unwrap();
        let archive = scratch.path().join("archive.tar");
        fs::write(&archive, bytes).unwrap();
        extract_archive(&archive, &scratch.path().join("staging"))
    }

    #[test]
    fn links_that_resolve_through_other_links_cannot_escape() {
        assert!(
            extract_tar(&tar_with_links(&[
                ("cef/d1", "."),
                ("cef/d1/d1/link", "../..")
            ]))
            .is_err()
        );
        assert!(extract_tar(&tar_with_links(&[("cef/link", "../..")])).is_err());
        assert!(extract_tar(&tar_with_links(&[("cef/d1", "."), ("cef/link", "d1/d1")])).is_ok());
    }

    #[test]
    fn extracts_the_zstd_tarballs_bundles_ship_as() {
        let mut builder = ::tar::Builder::new(Vec::new());
        let mut header = ::tar::Header::new_gnu();
        header.set_size(5);
        builder
            .append_data(&mut header, "app/notes", &b"notes"[..])
            .unwrap();
        let tarball = zstd::encode_all(&builder.into_inner().unwrap()[..], 19).unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let archive = scratch.path().join("app.tar.zst");
        fs::write(&archive, tarball).unwrap();

        extract_archive(&archive, &scratch.path().join("staging")).unwrap();

        assert_eq!(
            fs::read(scratch.path().join("staging/app/notes")).unwrap(),
            b"notes"
        );
    }
}
