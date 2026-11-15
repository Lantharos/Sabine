use std::{collections::BTreeMap, path::PathBuf};

/// The file in the app's profile that keeps people's permission choices.
const FILE: &str = "Sabine Permissions.json";

type Choices = BTreeMap<String, BTreeMap<String, bool>>;

/// What people allowed and denied each origin, kept in the app's profile so
/// they aren't asked again.
pub(in crate::osr::host) struct PermissionStore {
    path: Option<PathBuf>,
    choices: Option<Choices>,
}

impl PermissionStore {
    pub(in crate::osr::host) fn new(app_id: Option<&str>) -> Self {
        Self {
            path: app_id.map(|id| crate::host::browser_profile_dir(id).join(FILE)),
            choices: None,
        }
    }

    /// Denied when people denied any of `permissions`, allowed when they
    /// allowed all of them.
    pub(in crate::osr::host) fn decided(
        &mut self,
        origin: &str,
        permissions: &[String],
    ) -> Option<bool> {
        let chosen = self.choices().get(origin)?;
        let mut choices = permissions
            .iter()
            .map(|permission| chosen.get(permission).copied());
        if choices.clone().any(|choice| choice == Some(false)) {
            return Some(false);
        }
        choices.all(|choice| choice == Some(true)).then_some(true)
    }

    pub(in crate::osr::host) fn remember(
        &mut self,
        origin: &str,
        permissions: &[String],
        allowed: bool,
    ) {
        let chosen = self.choices().entry(origin.to_string()).or_default();
        for permission in permissions {
            chosen.insert(permission.clone(), allowed);
        }
        self.save();
    }

    fn choices(&mut self) -> &mut Choices {
        let path = self.path.as_ref();
        self.choices.get_or_insert_with(|| {
            path.and_then(|path| std::fs::read(path).ok())
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default()
        })
    }

    fn save(&self) {
        let (Some(path), Some(choices)) = (&self.path, &self.choices) else {
            return;
        };
        let staged = path.with_extension("json.new");
        let written = serde_json::to_vec_pretty(choices)
            .map_err(std::io::Error::other)
            .and_then(|bytes| std::fs::write(&staged, bytes))
            .and_then(|()| std::fs::rename(&staged, path));
        if let Err(error) = written {
            sabine_runtime::report_error(
                "permissions",
                format!("could not remember a permission choice: {error}"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_denial_outweighs_what_was_allowed() {
        let folder = tempfile::tempdir().unwrap();
        let mut store = PermissionStore {
            path: Some(folder.path().join(FILE)),
            choices: None,
        };
        let names = |names: &[&str]| names.iter().map(ToString::to_string).collect::<Vec<_>>();
        store.remember("https://meet.example", &names(&["camera"]), true);
        assert_eq!(
            store.decided("https://meet.example", &names(&["camera"])),
            Some(true)
        );
        assert_eq!(
            store.decided("https://meet.example", &names(&["camera", "microphone"])),
            None
        );
        store.remember("https://meet.example", &names(&["microphone"]), false);
        let mut reread = PermissionStore {
            path: Some(folder.path().join(FILE)),
            choices: None,
        };
        assert_eq!(
            reread.decided("https://meet.example", &names(&["camera", "microphone"])),
            Some(false)
        );
    }
}
