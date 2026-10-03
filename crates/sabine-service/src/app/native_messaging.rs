use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMessagingBrowser {
    Chromium,
    Firefox,
}

pub fn native_messaging_manifest_dirs() -> io::Result<Vec<(NativeMessagingBrowser, PathBuf)>> {
    use NativeMessagingBrowser::{Chromium, Firefox};
    #[cfg(target_os = "linux")]
    {
        let home = home_dir()?;
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|path| !path.is_empty())
            .map_or_else(|| home.join(".config"), PathBuf::from);
        Ok([
            "google-chrome",
            "chromium",
            "microsoft-edge",
            "BraveSoftware/Brave-Browser",
        ]
        .into_iter()
        .map(|browser| (Chromium, config.join(browser).join("NativeMessagingHosts")))
        .chain([(Firefox, home.join(".mozilla/native-messaging-hosts"))])
        .collect())
    }
    #[cfg(target_os = "macos")]
    {
        let support = home_dir()?.join("Library/Application Support");
        Ok([
            "Google/Chrome",
            "Chromium",
            "Microsoft Edge",
            "BraveSoftware/Brave-Browser",
        ]
        .into_iter()
        .map(|browser| (Chromium, support.join(browser).join("NativeMessagingHosts")))
        .chain([(Firefox, support.join("Mozilla/NativeMessagingHosts"))])
        .collect())
    }
    #[cfg(windows)]
    {
        let directory = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?
            .join("sabine")
            .join("native-messaging");
        Ok(vec![
            (Chromium, directory.join("chromium")),
            (Firefox, directory.join("firefox")),
        ])
    }
}

#[cfg(windows)]
pub fn native_messaging_registry_keys(browser: NativeMessagingBrowser) -> &'static [&'static str] {
    match browser {
        NativeMessagingBrowser::Chromium => &[
            r"Software\Google\Chrome\NativeMessagingHosts",
            r"Software\Chromium\NativeMessagingHosts",
            r"Software\Microsoft\Edge\NativeMessagingHosts",
            r"Software\BraveSoftware\Brave-Browser\NativeMessagingHosts",
        ],
        NativeMessagingBrowser::Firefox => &[r"Software\Mozilla\NativeMessagingHosts"],
    }
}

pub fn remove_native_messaging_hosts(install: &Path) -> io::Result<()> {
    for (_, directory) in native_messaging_manifest_dirs()? {
        remove_hosts_in(&directory, install)?;
    }
    Ok(())
}

fn remove_hosts_in(directory: &Path, install: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let manifest = entry?.path();
        if manifest
            .extension()
            .is_some_and(|extension| extension == "json")
            && launches_from(&manifest, install)
        {
            #[cfg(windows)]
            registry::unregister(&manifest)?;
            fs::remove_file(&manifest)?;
        }
    }
    Ok(())
}

fn launches_from(manifest: &Path, install: &Path) -> bool {
    fs::read(manifest)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|manifest| manifest.get("path")?.as_str().map(PathBuf::from))
        .is_some_and(|program| inside(&program, install))
}

#[cfg(unix)]
fn inside(program: &Path, install: &Path) -> bool {
    let (Ok(program), Ok(install)) = (program.canonicalize(), install.canonicalize()) else {
        return false;
    };
    program.starts_with(install)
}

#[cfg(windows)]
fn inside(program: &Path, install: &Path) -> bool {
    registry::folded(program).starts_with(registry::folded(install))
}

#[cfg(unix)]
fn home_dir() -> io::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

#[cfg(windows)]
mod registry {
    use super::{NativeMessagingBrowser, native_messaging_registry_keys};
    use std::{
        io,
        path::{Path, PathBuf},
    };
    use windows::{
        Win32::{
            Foundation::ERROR_SUCCESS,
            System::Registry::{HKEY_CURRENT_USER, RegDeleteTreeW},
        },
        core::HSTRING,
    };

    pub(super) fn unregister(manifest: &Path) -> io::Result<()> {
        let Some(host) = manifest.file_stem().and_then(|stem| stem.to_str()) else {
            return Ok(());
        };
        let bases = [
            NativeMessagingBrowser::Chromium,
            NativeMessagingBrowser::Firefox,
        ]
        .into_iter()
        .flat_map(native_messaging_registry_keys);
        for base in bases {
            let key = format!(r"{base}\{host}");
            let registered = crate::windows_registry::current_user_string(&key)
                .is_some_and(|value| folded(Path::new(&value)) == folded(manifest));
            if registered {
                let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(&key)) };
                if status != ERROR_SUCCESS {
                    return Err(io::Error::other(format!(
                        "could not remove HKCU\\{key}: {status:?}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub(super) fn folded(path: &Path) -> PathBuf {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_manifests_that_launch_this_install() {
        let root = std::env::temp_dir().join(format!(
            "sabine-native-messaging-test-{}",
            std::process::id()
        ));
        let install = root.join("apps/com.example.notes/install");
        let other = root.join("apps/com.example.notes-beta/install");
        let hosts = root.join("NativeMessagingHosts");
        for directory in [&install, &other, &hosts] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(install.join("notes-host"), "").unwrap();
        fs::write(other.join("notes-host"), "").unwrap();
        let manifest = |host: &str, program: &Path| {
            let path = hosts.join(format!("{host}.json"));
            fs::write(
                &path,
                serde_json::json!({ "name": host, "path": program, "type": "stdio" }).to_string(),
            )
            .unwrap();
            path
        };
        let ours = manifest("com.example.notes", &install.join("notes-host"));
        let taken_over = manifest("com.example.shared", &other.join("notes-host"));
        let unrelated = hosts.join("README.txt");
        fs::write(
            &unrelated,
            install.join("notes-host").to_string_lossy().as_bytes(),
        )
        .unwrap();

        remove_hosts_in(&hosts, &install).unwrap();

        assert!(!ours.exists());
        assert!(taken_over.exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn matches_registrations_written_with_mixed_separators() {
        assert_eq!(
            registry::folded(Path::new(
                r"C:\Users\Ana\AppData\Local\sabine/native-messaging\chromium\com.example.notes.json"
            )),
            registry::folded(Path::new(
                r"c:\users\ana\appdata\local\Sabine\native-messaging\chromium\com.example.notes.json"
            ))
        );
        assert!(inside(
            Path::new(
                r"c:\users\ana\appdata\local\sabine\apps\com.example.notes\install\notes.exe"
            ),
            Path::new(r"C:\Users\Ana\AppData\Local\Sabine\apps\com.example.notes\install"),
        ));
    }
}
