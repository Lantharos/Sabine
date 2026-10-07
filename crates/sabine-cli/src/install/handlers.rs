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
