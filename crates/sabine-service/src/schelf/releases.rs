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
    let releases = releases_of(install);
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

/// Removes every release of the app installed at `install` but `current`
/// whose `program` no process runs. Refuses a `current` outside the app's
/// releases folder.
pub(super) fn remove_unused_releases(
    install: &Path,
    current: &Path,
    program: &Path,
) -> io::Result<()> {
    let releases = releases_of(install);
    if current.parent() != Some(releases.as_path()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} isn't a release of {}",
                current.display(),
                install.display()
            ),
        ));
    }
    for entry in fs::read_dir(&releases)? {
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

fn releases_of(install: &Path) -> PathBuf {
    install.with_file_name(RELEASES)
}

/// Whether a process runs `program`: every Sabine app holds a shared lock on
/// the program it runs.
fn running(program: &Path) -> bool {
    File::open(program).is_ok_and(|file| file.try_lock().is_err())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_apps_own_releases_are_removed() {
        let base = tempfile::tempdir().unwrap();
        let install = base.path().join("app/install");
        let elsewhere = base.path().join("elsewhere");
        fs::create_dir_all(elsewhere.join("current")).unwrap();
        fs::create_dir_all(elsewhere.join("other")).unwrap();
        assert!(
            remove_unused_releases(&install, &elsewhere.join("current"), Path::new("app")).is_err()
        );
        assert!(elsewhere.join("other").is_dir());

        let current = release_folder(&install, "1.1.0").unwrap();
        let old = release_folder(&install, "1.0.0").unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&old).unwrap();
        remove_unused_releases(&install, &current, Path::new("app")).unwrap();
        assert!(current.is_dir());
        assert!(!old.exists());
    }
}
