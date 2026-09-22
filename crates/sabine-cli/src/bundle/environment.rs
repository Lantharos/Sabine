use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) struct BuildEnvironment(BTreeMap<String, String>);

impl BuildEnvironment {
    pub fn load(source: &Path, web_root: Option<&Path>, extra: &[PathBuf]) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        for root in std::iter::once(source).chain(web_root.filter(|root| *root != source)) {
            for name in [
                ".env",
                ".env.local",
                ".env.production",
                ".env.production.local",
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
        Ok(Self(values))
    }

    pub fn apply(&self, command: &mut Command) {
        command.envs(&self.0).env("NODE_ENV", "production");
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
