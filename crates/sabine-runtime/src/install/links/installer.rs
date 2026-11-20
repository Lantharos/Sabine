//! Windows installers copy every file they carry and can't hold links, so a
//! runtime staged for one lists its links in `.sabine-links` instead of
//! carrying them, and setup makes them once the app's files are in place.
//! The installers never own the links, so repairing an app can't write
//! through one into the file it shares.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

const LINKS: &str = ".sabine-links";

/// Each link and the file it links to, relative to their runtime.
type Links = Vec<(PathBuf, PathBuf)>;

/// Takes the links out of the runtime at `runtime_dir` and lists them for
/// setup to make.
#[cfg(windows)]
pub fn list_runtime_links(runtime_dir: &Path) -> io::Result<()> {
    let relative = |path: PathBuf| {
        path.strip_prefix(runtime_dir)
            .expect("runtime files are inside their runtime")
            .to_path_buf()
    };
    let mut links: Links = super::assets::RELEASE_ASSETS
        .iter()
        .map(|asset| {
            (
                Path::new("Release").join(asset),
                Path::new("Resources").join(asset),
            )
        })
        .collect();
    let english = relative(super::locales::english_pack(runtime_dir));
    for pack in super::locales::linked_packs(runtime_dir)? {
        links.push((relative(pack), english.clone()));
    }
    for (link, _) in &links {
        fs::remove_file(runtime_dir.join(link))?;
    }
    fs::write(runtime_dir.join(LINKS), serde_json::to_vec(&links)?)
}

/// The links the runtimes in `bundled_dir` list, relative to it: the files
/// setup adds and uninstalling removes.
pub fn listed_runtime_links(bundled_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for (runtime, links) in listed_links(bundled_dir)? {
        let runtime = runtime
            .strip_prefix(bundled_dir)
            .expect("bundled runtimes are inside their app");
        paths.extend(links.into_iter().map(|(link, _)| runtime.join(link)));
    }
    Ok(paths)
}

/// Makes the links the runtimes in `bundled_dir` list.
#[cfg(windows)]
pub fn link_bundled_runtimes(bundled_dir: &Path) -> io::Result<()> {
    for (runtime, links) in listed_links(bundled_dir)? {
        for (link, target) in links {
            let link = runtime.join(link);
            crate::remove_file_if_present(&link)?;
            fs::hard_link(runtime.join(target), link)?;
        }
    }
    Ok(())
}

fn listed_links(bundled_dir: &Path) -> io::Result<Vec<(PathBuf, Links)>> {
    let runtimes = crate::discovery::paths::bundled_runtime_path(bundled_dir);
    if !runtimes.is_dir() {
        return Ok(Vec::new());
    }
    let mut listed = Vec::new();
    for entry in fs::read_dir(runtimes)? {
        let runtime = entry?.path();
        let list = runtime.join(LINKS);
        if !list.is_file() {
            continue;
        }
        let links: Links = serde_json::from_slice(&fs::read(list)?)?;
        if links
            .iter()
            .any(|(link, target)| !inside(link) || !inside(target))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a runtime lists a link outside itself",
            ));
        }
        listed.push((runtime, links));
    }
    Ok(listed)
}

fn inside(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn setup_makes_the_links_the_installer_left_out() {
        let app = tempfile::tempdir().unwrap();
        let runtime = crate::discovery::paths::bundled_runtime_path(app.path()).join("cef");
        let locales = runtime.join("Resources/locales");
        fs::create_dir_all(&locales).unwrap();
        fs::create_dir_all(runtime.join("Release")).unwrap();
        for asset in super::super::assets::RELEASE_ASSETS {
            fs::write(runtime.join("Resources").join(asset), asset).unwrap();
            fs::hard_link(
                runtime.join("Resources").join(asset),
                runtime.join("Release").join(asset),
            )
            .unwrap();
        }
        fs::write(locales.join("en-US.pak"), "English").unwrap();
        fs::hard_link(locales.join("en-US.pak"), locales.join("de.pak")).unwrap();

        list_runtime_links(&runtime).unwrap();
        assert!(!locales.join("de.pak").exists());
        assert!(!runtime.join("Release/resources.pak").exists());
        assert_eq!(listed_runtime_links(app.path()).unwrap().len(), 5);

        link_bundled_runtimes(app.path()).unwrap();
        assert_eq!(
            fs::read_to_string(locales.join("de.pak")).unwrap(),
            "English"
        );
        assert_eq!(
            fs::read_to_string(runtime.join("Release/resources.pak")).unwrap(),
            "resources.pak"
        );
    }
}
