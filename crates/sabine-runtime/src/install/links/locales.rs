//! Chromium shows a little text of its own in pages, such as form validation
//! messages, from the locale pack of the system's language. Sabine apps
//! translate their own pages, so every pack is the English one, linked
//! rather than copied. Chromium still picks the system's locale, which keeps
//! the dates and numbers pages format the way people expect, and all the
//! packs together cost what one does.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[cfg(not(target_os = "macos"))]
const ENGLISH: &str = "en-US.pak";
#[cfg(target_os = "macos")]
const ENGLISH: &str = "en.lproj/locale.pak";

#[cfg(not(target_os = "macos"))]
fn locales_dir(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("Resources").join("locales")
}

#[cfg(target_os = "macos")]
fn locales_dir(runtime_dir: &Path) -> PathBuf {
    let resources = Path::new("Chromium Embedded Framework.framework/Resources");
    let framework = runtime_dir.join(resources);
    if framework.is_dir() {
        framework
    } else {
        runtime_dir.join("Release").join(resources)
    }
}

/// The runtime's English locale pack, which every other one links to.
pub(crate) fn english_pack(runtime_dir: &Path) -> PathBuf {
    locales_dir(runtime_dir).join(ENGLISH)
}

/// The runtime's locale packs other than the English one.
pub(crate) fn linked_packs(runtime_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let english = english_pack(runtime_dir);
    let mut packs = Vec::new();
    for entry in fs::read_dir(locales_dir(runtime_dir))? {
        packs.extend(pack_path(entry?.path()).filter(|path| path.is_file() && *path != english));
    }
    Ok(packs)
}

#[cfg(not(target_os = "macos"))]
fn pack_path(entry: PathBuf) -> Option<PathBuf> {
    entry
        .extension()
        .is_some_and(|extension| extension == "pak")
        .then_some(entry)
}

#[cfg(target_os = "macos")]
fn pack_path(entry: PathBuf) -> Option<PathBuf> {
    entry
        .extension()
        .is_some_and(|extension| extension == "lproj")
        .then(|| entry.join("locale.pak"))
}

/// Makes every locale pack of the runtime a hard link to the English one.
pub(crate) fn link_locale_packs(runtime_dir: &Path) -> io::Result<()> {
    let english = english_pack(runtime_dir);
    for path in linked_packs(runtime_dir)? {
        fs::remove_file(&path)?;
        fs::hard_link(&english, &path)?;
    }
    Ok(())
}

/// Links the locale packs other than the English one that `source_runtime`
/// has into `runtime_dir`, a copy of it that holds the rest of its files.
pub(crate) fn copy_locale_packs(source_runtime: &Path, runtime_dir: &Path) -> io::Result<()> {
    let english = english_pack(runtime_dir);
    for pack in linked_packs(source_runtime)? {
        let relative = pack
            .strip_prefix(source_runtime)
            .expect("locale packs are inside their runtime");
        let destination = runtime_dir.join(relative);
        fs::create_dir_all(destination.parent().expect("a locale pack has a folder"))?;
        fs::hard_link(&english, destination)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_with_packs(root: &Path) {
        for (pack, text) in [
            (english_pack(root), "English"),
            (pack_named(root, "de"), "Deutsch"),
            (pack_named(root, "fr"), "Français"),
        ] {
            fs::create_dir_all(pack.parent().unwrap()).unwrap();
            fs::write(pack, text).unwrap();
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn pack_named(root: &Path, locale: &str) -> PathBuf {
        locales_dir(root).join(format!("{locale}.pak"))
    }

    #[cfg(target_os = "macos")]
    fn pack_named(root: &Path, locale: &str) -> PathBuf {
        locales_dir(root).join(format!("{locale}.lproj/locale.pak"))
    }

    fn assert_every_pack_is_english(root: &Path) {
        for locale in ["de", "fr"] {
            assert_eq!(
                fs::read_to_string(pack_named(root, locale)).unwrap(),
                "English"
            );
        }
    }

    #[test]
    fn every_pack_holds_the_english_text() {
        let runtime = tempfile::tempdir().unwrap();
        runtime_with_packs(runtime.path());

        link_locale_packs(runtime.path()).unwrap();

        assert_every_pack_is_english(runtime.path());
    }

    #[test]
    fn copies_keep_every_locale_name() {
        let source = tempfile::tempdir().unwrap();
        let copy = tempfile::tempdir().unwrap();
        runtime_with_packs(source.path());
        let english = english_pack(copy.path());
        fs::create_dir_all(english.parent().unwrap()).unwrap();
        fs::copy(english_pack(source.path()), english).unwrap();

        copy_locale_packs(source.path(), copy.path()).unwrap();

        assert_every_pack_is_english(copy.path());
    }
}
