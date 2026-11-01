use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) struct BuildEnvironment {
    values: BTreeMap<String, String>,
    mode: sabine_service::AppEnvironment,
}

impl BuildEnvironment {
    pub fn load(
        source: &Path,
        web_root: Option<&Path>,
        extra: &[PathBuf],
        mode: sabine_service::AppEnvironment,
    ) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        for root in std::iter::once(source).chain(web_root.filter(|root| *root != source)) {
            for name in [
                ".env".to_owned(),
                ".env.local".to_owned(),
                format!(".env.{}", mode.as_str()),
                format!(".env.{}.local", mode.as_str()),
            ] {
                let path = root.join(name);
                if path.is_file() {
                    load_file(&path, &mut values)?;
                }
            }
        }
        for path in extra {
            load_file(&source.join(path), &mut values)?;
        }
        values.retain(|name, _| std::env::var_os(name).is_none());
        Ok(Self { values, mode })
    }

    pub fn apply(&self, command: &mut Command) {
        command
            .envs(&self.values)
            .env("NODE_ENV", self.mode.as_str())
            .env("SABINE_ENV", self.mode.as_str());
        if self.mode == sabine_service::AppEnvironment::Production {
            for key in [
                "SABINE_DEV_URL",
                "SABINE_APP_ID",
                "SABINE_WEB_ENTRY",
                "SABINE_MANIFEST_PATH",
            ] {
                command.env_remove(key);
            }
        }
    }
}

fn load_file(path: &Path, values: &mut BTreeMap<String, String>) -> Result<(), String> {
    let entries = dotenvy::from_path_iter(path)
        .map_err(|_| format!("could not read environment file {}", path.display()))?;
    for entry in entries {
        let (name, value) =
            entry.map_err(|_| format!("invalid environment file {}", path.display()))?;
        values.insert(name, value);
    }
    Ok(())
}
