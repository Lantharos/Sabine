// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Chromium subprocesses on Windows and Linux look beside libcef for ICU and
// .pak files even when the browser has explicit resource paths. Keep the
// Release hard links to Resources; removing these apparent duplicates breaks
// startup on Windows and leaves Linux renderers without Blink's resources.

use std::{io, path::Path};

#[cfg(any(target_os = "linux", windows))]
pub(crate) const RELEASE_ASSETS: [&str; 4] = [
    "icudtl.dat",
    "chrome_100_percent.pak",
    "chrome_200_percent.pak",
    "resources.pak",
];

pub fn prepare_runtime_assets(runtime_dir: &Path) -> io::Result<()> {
    #[cfg(any(target_os = "linux", windows))]
    link_release_assets(runtime_dir)?;
    #[cfg(windows)]
    crate::prepare_sandbox_access(runtime_dir, true).map_err(io::Error::other)?;
    #[cfg(not(any(target_os = "linux", windows)))]
    let _ = runtime_dir;
    Ok(())
}

#[cfg(any(target_os = "linux", windows))]
pub(crate) fn link_release_assets(runtime_dir: &Path) -> io::Result<()> {
    for asset in RELEASE_ASSETS {
        link_runtime_asset(runtime_dir, asset)?;
    }
    Ok(())
}

/// Links the files the runtimes an app ships share again, which its Windows
/// installer carried empty.
#[cfg(windows)]
pub fn link_bundled_runtimes(bundled_dir: &Path) -> io::Result<()> {
    let runtimes = crate::discovery::paths::bundled_runtime_path(bundled_dir);
    if !runtimes.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(runtimes)? {
        let runtime = entry?.path();
        if !crate::discovery::detect::is_runtime_dir(&runtime) {
            continue;
        }
        for asset in RELEASE_ASSETS {
            crate::remove_file_if_present(&runtime.join("Release").join(asset))?;
        }
        link_release_assets(&runtime)?;
        super::locales::link_locale_packs(&runtime)?;
    }
    Ok(())
}

/// Windows installers copy every file they carry, so a runtime staged for one
/// holds an empty file in place of each link, and
/// [`link_bundled_runtimes`] links them again once setup has put them in place.
#[cfg(windows)]
pub fn empty_runtime_links(runtime_dir: &Path) -> io::Result<()> {
    let assets = RELEASE_ASSETS
        .iter()
        .map(|asset| runtime_dir.join("Release").join(asset));
    for path in assets.chain(super::locales::linked_packs(runtime_dir)?) {
        std::fs::remove_file(&path)?;
        std::fs::File::create(&path)?;
    }
    Ok(())
}

#[cfg(any(target_os = "linux", windows))]
fn link_runtime_asset(runtime_dir: &Path, asset: &str) -> io::Result<()> {
    let target = runtime_dir.join("Release").join(asset);
    if !target.is_file() {
        let source = runtime_dir.join("Resources").join(asset);
        match std::fs::hard_link(&source, &target) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && target.is_file() => {}
            Err(error) => {
                return Err(io::Error::new(
                    error.kind(),
                    format!(
                        "could not prepare CEF resource at {}: {error}",
                        target.display()
                    ),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn setup_links_what_the_installer_carried_empty() {
        let runtime = tempfile::tempdir().unwrap();
        let root = runtime.path();
        let locales = root.join("Resources/locales");
        fs::create_dir_all(&locales).unwrap();
        fs::create_dir_all(root.join("Release")).unwrap();
        for asset in RELEASE_ASSETS {
            fs::write(root.join("Resources").join(asset), asset).unwrap();
        }
        fs::write(locales.join("en-US.pak"), "English").unwrap();
        fs::write(locales.join("de.pak"), "English").unwrap();
        link_release_assets(root).unwrap();

        empty_runtime_links(root).unwrap();
        assert_eq!(fs::read(locales.join("de.pak")).unwrap(), b"");
        assert_eq!(
            fs::read(root.join("Resources/resources.pak")).unwrap(),
            b"resources.pak"
        );

        for asset in RELEASE_ASSETS {
            crate::remove_file_if_present(&root.join("Release").join(asset)).unwrap();
        }
        link_release_assets(root).unwrap();
        super::super::locales::link_locale_packs(root).unwrap();
        assert_eq!(
            fs::read_to_string(locales.join("de.pak")).unwrap(),
            "English"
        );
        assert_eq!(
            fs::read_to_string(root.join("Release/resources.pak")).unwrap(),
            "resources.pak"
        );
    }
}
