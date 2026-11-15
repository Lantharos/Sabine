use std::{
    io,
    path::{Component, Path, PathBuf},
};

use super::invalid;

/// A symbolic link an archive holds, at `path` below its destination.
pub(super) struct Link {
    pub path: PathBuf,
    pub target: PathBuf,
}

/// Creates the archive's links after everything else, so no entry was written
/// through one, then requires each to resolve inside `destination`.
pub(super) fn create(destination: &Path, links: &[Link]) -> io::Result<()> {
    let mut created = Vec::with_capacity(links.len());
    for link in links {
        let full = destination.join(&link.path);
        let (Some(parent), Some(name)) = (full.parent(), full.file_name()) else {
            return Err(invalid("archive link escapes its installation"));
        };
        let parent = parent.canonicalize()?;
        if !parent.starts_with(destination) || !is_relative(&link.target) {
            return Err(invalid("archive link escapes its installation"));
        }
        let path = parent.join(name);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&link.target, &path)?;
        #[cfg(windows)]
        if links_to_directory(destination, link, links) {
            std::os::windows::fs::symlink_dir(&link.target, &path)?;
        } else {
            std::os::windows::fs::symlink_file(&link.target, &path)?;
        }
        created.push(path);
    }
    for path in created {
        if !path
            .canonicalize()
            .is_ok_and(|resolved| resolved.starts_with(destination))
        {
            return Err(invalid("archive link resolves outside its installation"));
        }
    }
    Ok(())
}

fn is_relative(target: &Path) -> bool {
    target.components().all(|part| {
        matches!(
            part,
            Component::Normal(_) | Component::CurDir | Component::ParentDir
        )
    })
}

/// Whether `link` ends at a directory, following the archive's other links,
/// which Windows needs to know before they exist.
#[cfg(windows)]
fn links_to_directory(destination: &Path, link: &Link, links: &[Link]) -> bool {
    let mut current = destination.join(&link.path);
    for _ in 0..=links.len() {
        let Some(link) = links
            .iter()
            .find(|link| destination.join(&link.path) == current)
        else {
            return current.is_dir();
        };
        let mut next = current.parent().expect("links have a parent").to_path_buf();
        for part in link.target.components() {
            match part {
                Component::ParentDir => {
                    next.pop();
                }
                Component::Normal(name) => next.push(name),
                _ => {}
            }
        }
        current = next;
    }
    false
}
