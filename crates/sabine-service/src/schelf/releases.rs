//! A store app installs each version into a folder of its own under
//! `releases`, and `install` links to the one the app launches. A version
//! another process still runs keeps its folder, so the process goes on
//! finding its own program, pages and resources until it ends.

use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};

use sabine_runtime::{random_hex, sync_directory};

const RELEASES: &str = "releases";

/// A new folder for `version` of the app installed at `install`, also when
/// the same version is installed again.
pub(super) fn release_folder(install: &Path, version: &str) -> io::Result<PathBuf> {
    let releases = install.with_file_name(RELEASES);
    let folder = releases.join(version);
    if fs::symlink_metadata(&folder).is_err() {
        return Ok(folder);
    }
    Ok(releases.join(format!("{version}-{}", random_hex(4)?)))
}

/// Makes `install` link to `release`. A copy installed into `install` itself
/// moves beside the releases first, to be removed once nothing runs it.
pub(super) fn launch_release(install: &Path, release: &Path) -> io::Result<()> {
    let releases = release
        .parent()
        .expect("a release sits in the releases folder");
    if fs::symlink_metadata(install).is_ok_and(|metadata| metadata.is_dir()) {
        fs::rename(install, releases.join(random_hex(8)?))?;
    }
    let link = install.with_file_name(".install.link");
    sabine_runtime::remove_file_if_present(&link)?;
    std::os::unix::fs::symlink(
        Path::new(RELEASES).join(release.file_name().expect("a release has a name")),
        &link,
    )?;
    fs::rename(&link, install)?;
    sync_directory(install.parent().expect("an installation has a folder"))
}

/// Removes every release but `current` whose `program` no process runs.
pub(super) fn remove_unused_releases(current: &Path, program: &Path) -> io::Result<()> {
    let releases = current
        .parent()
        .expect("a release sits in the releases folder");
    for entry in fs::read_dir(releases)? {
        let entry = entry?;
        let path = entry.path();
        if path == current
            || entry.file_name().to_string_lossy().starts_with('.')
            || !entry.file_type()?.is_dir()
            || running(&path.join(program))
        {
            continue;
        }
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

/// Whether a process runs `program`: every Sabine app holds a shared lock on
/// the program it runs.
fn running(program: &Path) -> bool {
    File::open(program).is_ok_and(|file| file.try_lock().is_err())
}
