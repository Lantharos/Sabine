//! Chromium shows a little text of its own in pages, such as form validation
//! messages, from the locale pack of the system's language. Sabine apps
//! translate their own pages, so every pack is the English one, linked
//! rather than copied. Chromium still picks the system's locale, which keeps
//! the dates and numbers pages format the way people expect, and all the
//! packs together cost what one does.

use std::{fs, io, path::Path};

const ENGLISH: &str = "en-US.pak";

fn locales_dir(runtime_dir: &Path) -> std::path::PathBuf {
    runtime_dir.join("Resources").join("locales")
}

/// Makes every locale pack of the runtime a hard link to the English one.
pub(crate) fn link_locale_packs(runtime_dir: &Path) -> io::Result<()> {
    let locales = locales_dir(runtime_dir);
    let english = locales.join(ENGLISH);
    for entry in fs::read_dir(&locales)? {
        let path = entry?.path();
        if is_pack(&path) && path != english {
            fs::remove_file(&path)?;
            fs::hard_link(&english, &path)?;
        }
    }
    Ok(())
}

/// Copies the runtime's locale packs as hard links to its English one.
pub(crate) fn copy_locale_packs(source_runtime: &Path, runtime_dir: &Path) -> io::Result<()> {
    let source = locales_dir(source_runtime);
    let locales = locales_dir(runtime_dir);
    fs::create_dir_all(&locales)?;
    let english = locales.join(ENGLISH);
    fs::copy(source.join(ENGLISH), &english)?;
    for entry in fs::read_dir(&source)? {
        let path = entry?.path();
        let Some(name) = path.file_name().filter(|name| *name != ENGLISH) else {
            continue;
        };
        if is_pack(&path) {
            fs::hard_link(&english, locales.join(name))?;
        } else if path.is_file() {
            fs::copy(&path, locales.join(name))?;
        }
    }
    Ok(())
}

fn is_pack(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "pak") && path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_with_packs(root: &Path) {
        let locales = locales_dir(root);
        fs::create_dir_all(&locales).unwrap();
        fs::write(locales.join(ENGLISH), "English").unwrap();
        fs::write(locales.join("de.pak"), "Deutsch").unwrap();
        fs::write(locales.join("fr.pak"), "Français").unwrap();
    }

    #[test]
    fn every_pack_holds_the_english_text() {
        let runtime = tempfile::tempdir().unwrap();
        runtime_with_packs(runtime.path());

        link_locale_packs(runtime.path()).unwrap();

        let locales = locales_dir(runtime.path());
        for pack in ["de.pak", "fr.pak", ENGLISH] {
            assert_eq!(fs::read_to_string(locales.join(pack)).unwrap(), "English");
        }
    }

    #[test]
    fn copies_keep_every_locale_name() {
        let source = tempfile::tempdir().unwrap();
        let copy = tempfile::tempdir().unwrap();
        runtime_with_packs(source.path());

        copy_locale_packs(source.path(), copy.path()).unwrap();

        let locales = locales_dir(copy.path());
        for pack in ["de.pak", "fr.pak", ENGLISH] {
            assert_eq!(fs::read_to_string(locales.join(pack)).unwrap(), "English");
        }
    }
}
