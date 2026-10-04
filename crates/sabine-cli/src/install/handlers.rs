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
    use crate::{
        desktop::types::{Associations, prog_id},
        install::source::SourceApp,
    };
    use sabine_service::windows_registry::{
        current_user_string, delete_current_user_key, delete_current_user_value, path_within,
        set_current_user_value,
    };
    use std::path::Path;

    pub(in crate::install) fn register(app: &SourceApp, executable: &Path) -> Result<(), String> {
        let command = format!("\"{}\" \"%1\"", executable.display());
        for scheme in app.associations.schemes() {
            let key = format!(r"Software\Classes\{scheme}");
            set(&key, "", &format!("URL:{scheme}"))?;
            set(&key, "URL Protocol", "")?;
            set(&format!(r"{key}\shell\open\command"), "", &command)?;
        }
        if app.associations.extensions().next().is_none() {
            return Ok(());
        }
        let prog_id = prog_id(&app.id);
        let key = format!(r"Software\Classes\{prog_id}");
        set(&key, "", &format!("{} document", app.name))?;
        set(
            &format!(r"{key}\DefaultIcon"),
            "",
            &format!("\"{}\",0", executable.display()),
        )?;
        set(&format!(r"{key}\shell\open\command"), "", &command)?;
        for extension in app.associations.extensions() {
            set(
                &format!(r"Software\Classes\.{extension}\OpenWithProgids"),
                &prog_id,
                "",
            )?;
        }
        Ok(())
    }

    pub(in crate::install) fn remove(install: &Path) -> Result<(), String> {
        let Ok(manifest) = std::fs::read_to_string(install.join("resources/Sabine.toml")) else {
            return Ok(());
        };
        let Some((id, associations)) = manifest_associations(&manifest) else {
            return Ok(());
        };
        for scheme in associations.schemes() {
            let key = format!(r"Software\Classes\{scheme}");
            if launches_from(&key, install) {
                delete_current_user_key(&key).map_err(|error| error.to_string())?;
            }
        }
        let prog_id = prog_id(&id);
        let key = format!(r"Software\Classes\{prog_id}");
        if launches_from(&key, install) {
            for extension in associations.extensions() {
                delete_current_user_value(
                    &format!(r"Software\Classes\.{extension}\OpenWithProgids"),
                    &prog_id,
                )
                .map_err(|error| error.to_string())?;
            }
            delete_current_user_key(&key).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn set(key: &str, name: &str, value: &str) -> Result<(), String> {
        set_current_user_value(key, name, value).map_err(|error| error.to_string())
    }

    fn launches_from(key: &str, install: &Path) -> bool {
        current_user_string(&format!(r"{key}\shell\open\command"))
            .as_deref()
            .and_then(command_program)
            .is_some_and(|program| path_within(Path::new(program), install))
    }

    pub(super) fn manifest_associations(manifest: &str) -> Option<(String, Associations)> {
        #[derive(serde::Deserialize)]
        struct Manifest {
            app: App,
        }
        #[derive(serde::Deserialize)]
        struct App {
            id: String,
            #[serde(flatten)]
            associations: Associations,
        }
        let manifest = toml::from_str::<Manifest>(manifest).ok()?;
        Some((manifest.app.id, manifest.app.associations))
    }

    pub(super) fn command_program(command: &str) -> Option<&str> {
        command
            .strip_prefix('"')?
            .split_once('"')
            .map(|(program, _)| program)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn removes_only_handlers_that_launch_this_install() {
            let install =
                Path::new(r"C:\Users\Ana\AppData\Local\Sabine\apps\com.example.signin\install");
            let ours = r#""c:\users\ana\appdata\local\sabine\apps\com.example.signin\install\signin.exe" "%1""#;
            let other = r#""C:\Program Files\Other\other.exe" "%1""#;
            assert!(
                command_program(ours)
                    .is_some_and(|program| path_within(Path::new(program), install))
            );
            assert!(
                !command_program(other)
                    .is_some_and(|program| path_within(Path::new(program), install))
            );
            let (id, associations) = manifest_associations(
                "[app]\nid = \"com.example.signin\"\nmime_types = [\"text/plain\", \"x-scheme-handler/example-signin\"]\n\n[app.extensions]\n\"text/plain\" = [\"txt\"]\n",
            )
            .unwrap();
            assert_eq!(id, "com.example.signin");
            assert_eq!(
                associations.schemes().collect::<Vec<_>>(),
                ["example-signin"]
            );
            assert_eq!(associations.extensions().collect::<Vec<_>>(), ["txt"]);
        }
    }
}
