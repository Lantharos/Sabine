use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

use super::{Expansion, invalid, links::Link, safe_path};

/// Unpacks every entry of a zip archive except its symbolic links, which it
/// returns to be created once nothing else can be written through them.
pub(super) fn extract(input: File, destination: &Path) -> io::Result<Vec<Link>> {
    let mut archive = ::zip::ZipArchive::new(input)?;
    let mut expansion = Expansion::default();
    let mut links = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let path = PathBuf::from(entry.name()?.as_ref());
        if !safe_path(&path) {
            return Err(invalid("archive contains an unsafe path"));
        }
        let mode = entry.unix_mode().unwrap_or(0o644);
        if !matches!(mode & 0o170000, 0 | 0o100000 | 0o040000 | 0o120000) {
            return Err(invalid("archive contains an unsupported entry type"));
        }
        let size = entry.size();
        expansion.add(size)?;
        let output = destination.join(&path);
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }
        fs::create_dir_all(
            output
                .parent()
                .ok_or_else(|| invalid("entry has no parent"))?,
        )?;
        if entry.is_symlink() {
            if size > 4096 {
                return Err(invalid("archive symlink target is too long"));
            }
            let mut target = String::new();
            (&mut entry).take(size + 1).read_to_string(&mut target)?;
            if target.len() as u64 != size {
                return Err(invalid("archive entry size does not match its metadata"));
            }
            links.push(Link {
                path,
                target: target.into(),
            });
        } else {
            let mut file = File::create_new(&output)?;
            if io::copy(&mut (&mut entry).take(size + 1), &mut file)? != size {
                return Err(invalid("archive entry size does not match its metadata"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(mode & 0o777))?;
            }
        }
    }
    Ok(links)
}
