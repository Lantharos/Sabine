#[cfg(target_os = "linux")]
pub(super) mod linux {
    use std::{fs, io, path::Path, time::Duration};

    pub(in crate::install) fn remove(id: &str, install: &Path) -> Result<(), String> {
        let applications = crate::install::source::data_home()?.join("applications");
        let url_handler = applications.join(format!("{id}.sabine-url.desktop"));
        let mut desktop_ids = vec![format!("{id}.desktop")];
        if launches_from(&url_handler, install) {
            fs::remove_file(&url_handler).map_err(|error| error.to_string())?;
            desktop_ids.push(format!("{id}.sabine-url.desktop"));
        }
        forget_associations(&desktop_ids)
    }

    fn launches_from(entry: &Path, install: &Path) -> bool {
        let (Ok(entry), Ok(install)) = (fs::read_to_string(entry), install.canonicalize()) else {
            return false;
        };
        exec_program(&entry).is_some_and(|program| Path::new(&program).starts_with(install))
    }

    fn forget_associations(desktop_ids: &[String]) -> Result<(), String> {
        let config = crate::install::source::config_home()?;
        let _lock = sabine_runtime::FileLock::acquire(
            &config.join("sabine/mimeapps.lock"),
            Duration::from_secs(5),
            |_| {},
        )
        .map_err(|error| error.to_string())?;
        let path = config.join("mimeapps.list");
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.to_string()),
        };
        let updated = without_desktop_ids(&content, desktop_ids);
        if updated == content {
            return Ok(());
        }
        let temporary = path.with_extension("list.sabine-tmp");
        fs::write(&temporary, updated).map_err(|error| error.to_string())?;
        fs::rename(temporary, path).map_err(|error| error.to_string())
    }

    pub(super) fn exec_program(entry: &str) -> Option<String> {
        let value = entry.lines().find_map(|line| line.strip_prefix("Exec="))?;
        let mut characters = value.replace("\\\\", "\\").into_bytes().into_iter();
        if characters.next()? != b'"' {
            return None;
        }
        let mut program = Vec::new();
        while let Some(byte) = characters.next() {
            match byte {
                b'\\' => program.push(characters.next()?),
                b'"' => {
                    return String::from_utf8(program)
                        .ok()
                        .map(|program| program.replace("%%", "%"));
                }
                _ => program.push(byte),
            }
        }
        None
    }

    pub(super) fn without_desktop_ids(content: &str, desktop_ids: &[String]) -> String {
        let mut output = String::with_capacity(content.len());
        for line in content.lines() {
            let owned = |entry: &&str| desktop_ids.iter().any(|id| id == entry);
            match line.split_once('=') {
                Some((key, value)) if value.split(';').any(|entry| owned(&entry)) => {
                    let remaining = value
                        .split(';')
                        .filter(|entry| !entry.is_empty() && !owned(entry))
                        .collect::<Vec<_>>();
                    if !remaining.is_empty() {
                        output.push_str(&format!("{key}={};\n", remaining.join(";")));
                    }
                }
                _ => {
                    output.push_str(line);
                    output.push('\n');
                }
            }
        }
        if !content.ends_with('\n') {
            output.pop();
        }
        output
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_the_program_the_runtime_writes() {
            let entry = "[Desktop Entry]\nType=Application\nExec=\"/home/a b/sabine/apps/com.example.signin/install/sign\\\\\"in\" %U\nNoDisplay=true\n";
            assert_eq!(
                exec_program(entry).as_deref(),
                Some("/home/a b/sabine/apps/com.example.signin/install/sign\"in")
            );
        }

        #[test]
        fn forgets_only_the_apps_associations() {
            let content = "[Default Applications]\nx-scheme-handler/example=com.example.signin.sabine-url.desktop\ntext/plain=org.gnome.TextEditor.desktop\n\n[Added Associations]\ntext/markdown=com.example.signin.desktop;org.gnome.TextEditor.desktop;\nx-scheme-handler/other=com.example.signin.desktop.extra.desktop;\n";
            let ids = [
                "com.example.signin.desktop".to_string(),
                "com.example.signin.sabine-url.desktop".to_string(),
            ];
            assert_eq!(
                without_desktop_ids(content, &ids),
                "[Default Applications]\ntext/plain=org.gnome.TextEditor.desktop\n\n[Added Associations]\ntext/markdown=org.gnome.TextEditor.desktop;\nx-scheme-handler/other=com.example.signin.desktop.extra.desktop;\n"
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) mod macos {
    use std::path::{Path, PathBuf};

    pub(in crate::install) fn remove(id: &str, install: &Path) {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        let bundles = [
            home.join("Applications").join(format!("{id}.app")),
            install.join(format!("{id}.app")),
        ];
        for bundle in bundles.iter().filter(|bundle| bundle.exists()) {
            let _ = std::process::Command::new(crate::install::desktop::LSREGISTER)
                .arg("-u")
                .arg(bundle)
                .status();
        }
    }
}

#[cfg(windows)]
pub(super) mod windows {
    use ::windows::{
        Win32::{
            Foundation::ERROR_SUCCESS,
            System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegDeleteTreeW, RegGetValueW},
        },
        core::HSTRING,
    };
    use std::path::Path;

    pub(in crate::install) fn remove(install: &Path) -> Result<(), String> {
        let Ok(manifest) = std::fs::read_to_string(install.join("resources/Sabine.toml")) else {
            return Ok(());
        };
        for scheme in manifest_schemes(&manifest) {
            let key = format!(r"Software\Classes\{scheme}");
            let launches_app = registry_string(&format!(r"{key}\shell\open\command"))
                .as_deref()
                .and_then(command_program)
                .is_some_and(|program| inside(program, install));
            if launches_app {
                let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(&key)) };
                if status != ERROR_SUCCESS {
                    return Err(format!("could not remove HKCU\\{key}: {status:?}"));
                }
            }
        }
        Ok(())
    }

    fn registry_string(key: &str) -> Option<String> {
        let key = HSTRING::from(key);
        let mut size = 0u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                None,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let mut buffer = vec![0u16; size as usize / 2];
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                None,
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        (status == ERROR_SUCCESS).then(|| {
            let length = buffer
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(buffer.len());
            String::from_utf16_lossy(&buffer[..length])
        })
    }

    pub(super) fn manifest_schemes(manifest: &str) -> Vec<String> {
        let mime_types: Vec<String> = toml::from_str::<toml::Table>(manifest)
            .ok()
            .and_then(|manifest| {
                manifest
                    .get("app")?
                    .get("mime_types")?
                    .clone()
                    .try_into()
                    .ok()
            })
            .unwrap_or_default();
        crate::desktop::types::schemes(&mime_types)
            .map(str::to_owned)
            .collect()
    }

    pub(super) fn command_program(command: &str) -> Option<&str> {
        command
            .strip_prefix('"')?
            .split_once('"')
            .map(|(program, _)| program)
    }

    pub(super) fn inside(program: &str, install: &Path) -> bool {
        let lowercase = |path: &Path| {
            std::path::PathBuf::from(dunce::simplified(path).to_string_lossy().to_lowercase())
        };
        lowercase(Path::new(program)).starts_with(lowercase(install))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn removes_only_schemes_that_launch_this_install() {
            let install =
                Path::new(r"C:\Users\Ana\AppData\Local\Sabine\apps\com.example.signin\install");
            let ours = r#""c:\users\ana\appdata\local\sabine\apps\com.example.signin\install\signin.exe" "%1""#;
            let other = r#""C:\Program Files\Other\other.exe" "%1""#;
            assert!(command_program(ours).is_some_and(|program| inside(program, install)));
            assert!(!command_program(other).is_some_and(|program| inside(program, install)));
            assert_eq!(
                manifest_schemes(
                    "[app]\nmime_types = [\"text/plain\", \"x-scheme-handler/example-signin\"]\n"
                ),
                ["example-signin"]
            );
        }
    }
}
