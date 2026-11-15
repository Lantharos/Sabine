use std::{fs, io, path::Path};

use super::{Expansion, invalid, links::Link, safe_path};

/// Unpacks every entry of a tar stream except its symbolic links, which it
/// returns to be created once nothing else can be written through them.
pub(super) fn extract(input: impl io::Read, destination: &Path) -> io::Result<Vec<Link>> {
    let mut archive = tar::Archive::new(input);
    let mut expansion = Expansion::default();
    let mut links = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if !safe_path(&path) {
            return Err(invalid("archive contains an unsafe path"));
        }
        expansion.add(entry.size())?;
        let kind = entry.header().entry_type();
        let target = entry.link_name()?.map(|target| target.into_owned());
        if kind.is_symlink() {
            let target = target.ok_or_else(|| invalid("archive link has no target"))?;
            if let Some(parent) = destination.join(&path).parent() {
                fs::create_dir_all(parent)?;
            }
            links.push(Link { path, target });
            continue;
        }
        if !(kind.is_file() || kind.is_dir() || kind.is_hard_link()) {
            return Err(invalid("archive contains an unsupported entry type"));
        }
        if kind.is_hard_link() && !target.is_some_and(|target| safe_path(&target)) {
            return Err(invalid("archive hard link escapes its installation"));
        }
        if !entry.unpack_in(destination)? {
            return Err(invalid(
                "archive entry could not be confined to its installation",
            ));
        }
    }
    Ok(links)
}
