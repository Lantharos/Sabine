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

/// Turns the copies a Windows installer makes of the files the runtimes an
/// app ships share back into hard links, as the runtimes were bundled.
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
