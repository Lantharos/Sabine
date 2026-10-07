use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMessagingBrowser {
    Chromium,
    Firefox,
}

#[cfg(target_os = "linux")]
const CHROMIUM_CONFIG_DIRS: &[&str] = &[
    "google-chrome",
    "google-chrome-beta",
    "google-chrome-unstable",
    "google-chrome-canary",
    "chromium",
    "microsoft-edge",
    "microsoft-edge-beta",
    "microsoft-edge-dev",
    "BraveSoftware/Brave-Browser",
    "BraveSoftware/Brave-Browser-Beta",
    "BraveSoftware/Brave-Browser-Nightly",
    "vivaldi",
    "vivaldi-snapshot",
    "opera",
    "net.imput.helium",
];

#[cfg(target_os = "linux")]
const FIREFOX_HOME_DIRS: &[&str] = &[".mozilla", ".librewolf"];

/// Flatpak browsers keep their configuration under `~/.var/app/<app id>/`.
#[cfg(target_os = "linux")]
const FLATPAK_BROWSERS: &[(&str, NativeMessagingBrowser, &str)] = &[
    (
        "com.google.Chrome",
        NativeMessagingBrowser::Chromium,
        "config/google-chrome",
    ),
    (
        "com.google.ChromeDev",
        NativeMessagingBrowser::Chromium,
        "config/google-chrome-unstable",
    ),
    (
        "org.chromium.Chromium",
        NativeMessagingBrowser::Chromium,
        "config/chromium",
    ),
    (
        "io.github.ungoogled_software.ungoogled_chromium",
        NativeMessagingBrowser::Chromium,
        "config/chromium",
    ),
    (
        "com.microsoft.Edge",
        NativeMessagingBrowser::Chromium,
        "config/microsoft-edge",
    ),
    (
        "com.brave.Browser",
        NativeMessagingBrowser::Chromium,
        "config/BraveSoftware/Brave-Browser",
    ),
    (
        "com.vivaldi.Vivaldi",
        NativeMessagingBrowser::Chromium,
        "config/vivaldi",
    ),
    (
        "com.opera.Opera",
        NativeMessagingBrowser::Chromium,
        "config/opera",
    ),
    (
        "org.mozilla.firefox",
        NativeMessagingBrowser::Firefox,
        ".mozilla",
    ),
    (
        "io.gitlab.librewolf-community",
        NativeMessagingBrowser::Firefox,
        ".librewolf",
    ),
    (
        "app.zen_browser.zen",
        NativeMessagingBrowser::Firefox,
        ".mozilla",
    ),
];

/// Snap browsers keep their configuration under `~/snap/<snap name>/common/`.
#[cfg(target_os = "linux")]
const SNAP_BROWSERS: &[(&str, NativeMessagingBrowser, &str)] = &[
    ("chromium", NativeMessagingBrowser::Chromium, "chromium"),
    ("firefox", NativeMessagingBrowser::Firefox, ".mozilla"),
];

pub fn native_messaging_manifest_dirs() -> io::Result<Vec<(NativeMessagingBrowser, PathBuf)>> {
    use NativeMessagingBrowser::{Chromium, Firefox};
    #[cfg(target_os = "linux")]
    {
        let home = super::home_dir()?;
        let config = super::config_home()?;
        let native = CHROMIUM_CONFIG_DIRS
            .iter()
            .map(|browser| (Chromium, config.join(browser)))
            .chain(
                FIREFOX_HOME_DIRS
                    .iter()
                    .map(|browser| (Firefox, home.join(browser))),
            );
        let flatpak = FLATPAK_BROWSERS.iter().filter_map(|(id, browser, config)| {
            let root = home.join(".var/app").join(id);
            root.is_dir().then(|| (*browser, root.join(config)))
        });
        let snap = SNAP_BROWSERS.iter().filter_map(|(name, browser, config)| {
            let root = home.join("snap").join(name).join("common");
            root.is_dir().then(|| (*browser, root.join(config)))
        });
        Ok(native
            .chain(flatpak)
            .chain(snap)
            .map(|(browser, root)| (browser, root.join(manifest_folder(browser))))
            .collect())
    }
    #[cfg(target_os = "macos")]
    {
        let support = super::home_dir()?.join("Library/Application Support");
        Ok([
            "Google/Chrome",
            "Google/Chrome Beta",
            "Google/Chrome Dev",
            "Google/Chrome Canary",
            "Chromium",
            "Microsoft Edge",
            "Microsoft Edge Beta",
            "Microsoft Edge Dev",
            "Microsoft Edge Canary",
            "BraveSoftware/Brave-Browser",
            "BraveSoftware/Brave-Browser-Beta",
            "BraveSoftware/Brave-Browser-Nightly",
            "Vivaldi",
            "com.operasoftware.Opera",
            "net.imput.helium",
        ]
        .into_iter()
        .map(|browser| (Chromium, support.join(browser).join("NativeMessagingHosts")))
        .chain(
            ["Mozilla", "LibreWolf"]
                .into_iter()
                .map(|browser| (Firefox, support.join(browser).join("NativeMessagingHosts"))),
        )
        .collect())
    }
    #[cfg(windows)]
    {
        let directory = sabine_runtime::sabine_data_dir().join("native-messaging");
        Ok(vec![
            (Chromium, directory.join("chromium")),
            (Firefox, directory.join("firefox")),
        ])
    }
}

#[cfg(target_os = "linux")]
fn manifest_folder(browser: NativeMessagingBrowser) -> &'static str {
    match browser {
        NativeMessagingBrowser::Chromium => "NativeMessagingHosts",
        NativeMessagingBrowser::Firefox => "native-messaging-hosts",
    }
}

/// Registry keys each browser family reads. Chrome's beta, dev and canary
/// channels share Chrome's key, Edge's channels share Edge's, and Firefox
/// forks such as LibreWolf and Zen read Mozilla's.
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

pub(crate) fn remove_native_messaging_hosts(installs: &[&Path]) -> io::Result<()> {
    for (_, directory) in native_messaging_manifest_dirs()? {
        remove_hosts_in(&directory, installs)?;
    }
    Ok(())
}

fn remove_hosts_in(directory: &Path, installs: &[&Path]) -> io::Result<()> {
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
            && installs
                .iter()
                .any(|install| launches_from(&manifest, install))
        {
            #[cfg(windows)]
            unregister(&manifest)?;
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
    program.starts_with(install)
        || matches!(
            (program.canonicalize(), install.canonicalize()),
            (Ok(program), Ok(install)) if program.starts_with(&install)
        )
}

#[cfg(windows)]
fn inside(program: &Path, install: &Path) -> bool {
    crate::windows_registry::path_within(program, install)
}

#[cfg(windows)]
fn unregister(manifest: &Path) -> io::Result<()> {
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
            .is_some_and(|value| crate::windows_registry::same_path(Path::new(&value), manifest));
        if registered {
            crate::windows_registry::delete_current_user_key(&key)?;
        }
    }
    Ok(())
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

        remove_hosts_in(&hosts, &[&install]).unwrap();

        assert!(!ours.exists());
        assert!(taken_over.exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
