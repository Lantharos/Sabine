use std::path::{Path, PathBuf};

use super::{data::browser_profile_path, forget::remove_path, registry::RegistryLock};
use crate::{RegisteredApp, SabineService, ServiceResult, types::unix_timestamp};

const DAY: u64 = 24 * 60 * 60;
/// Leaves room for an app on a drive that is not mounted right now.
const MISSING_GRACE: u64 = 7 * DAY;
const DEVELOPMENT_IDLE: u64 = 30 * DAY;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppCleanup {
    pub removed: Vec<String>,
    pub failures: Vec<String>,
}

impl SabineService {
    /// Removes apps that are gone for good: those whose program has been
    /// missing for a week, and development builds not launched for a month.
    /// Each loses its registration and everything Sabine keeps for it,
    /// including its browser profile.
    pub fn clean_up_apps(&self) -> ServiceResult<AppCleanup> {
        let mut cleanup = AppCleanup::default();
        for app in self.stale_apps()? {
            let id = app.manifest.id.clone();
            match self.remove_stale_app(&app) {
                Ok(()) => cleanup.removed.push(id),
                Err(error) => cleanup.failures.push(format!("{id}: {error}")),
            }
        }
        Ok(cleanup)
    }

    fn stale_apps(&self) -> ServiceResult<Vec<RegisteredApp>> {
        let _lock = RegistryLock::acquire(&self.root)?;
        let mut registry = self.load_registry()?;
        let now = unix_timestamp();
        let mut changed = false;
        let mut stale = Vec::new();
        for app in registry.apps.values_mut() {
            let missing_since =
                (!app.manifest.executable.exists()).then(|| app.missing_since.unwrap_or(now));
            changed |= missing_since != app.missing_since;
            app.missing_since = missing_since;
            if is_stale(app, now) {
                stale.push(app.clone());
            }
        }
        if changed {
            self.save_registry(&registry)?;
        }
        Ok(stale)
    }

    fn remove_stale_app(&self, app: &RegisteredApp) -> ServiceResult<()> {
        let id = &app.manifest.id;
        let directory = self.root.join("apps").join(id);
        let program_root = program_root(&app.manifest.executable, &directory);
        #[cfg(target_os = "linux")]
        super::desktop::entries::forget_desktop_entries(id, &program_root)?;
        self.forget_app(id, &program_root)?;
        remove_path(&directory)?;
        if let Some(profile) = browser_profile_path(id).parent() {
            remove_path(profile)?;
        }
        Ok(())
    }
}

fn is_stale(app: &RegisteredApp, now: u64) -> bool {
    let missing = app
        .missing_since
        .is_some_and(|since| now.saturating_sub(since) >= MISSING_GRACE);
    let idle_development =
        app.manifest.id.ends_with(".dev") && now.saturating_sub(app.updated_at) >= DEVELOPMENT_IDLE;
    missing || idle_development
}

/// Where the app's own programs live: its managed installation, or the folder
/// its executable sits in.
fn program_root(executable: &Path, directory: &Path) -> PathBuf {
    if executable.starts_with(directory) {
        return directory.join("install");
    }
    executable
        .parent()
        .map_or_else(|| executable.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppManifest;

    fn app(id: &str, updated_at: u64, missing_since: Option<u64>) -> RegisteredApp {
        RegisteredApp {
            manifest: AppManifest {
                id: id.to_string(),
                name: "Example".to_string(),
                version: "1.0.0".to_string(),
                executable: PathBuf::from("/opt/example/example"),
                args: Vec::new(),
                update: None,
                sabine: Default::default(),
            },
            registered_at: 0,
            updated_at,
            missing_since,
        }
    }

    #[test]
    fn apps_missing_for_a_week_are_stale() {
        let now = 100 * DAY;
        assert!(!is_stale(
            &app("com.example", now, Some(now - 6 * DAY)),
            now
        ));
        assert!(is_stale(&app("com.example", now, Some(now - 7 * DAY)), now));
    }

    #[test]
    fn only_development_builds_go_stale_from_disuse() {
        let now = 100 * DAY;
        assert!(is_stale(&app("com.example.dev", now - 30 * DAY, None), now));
        assert!(!is_stale(
            &app("com.example.dev", now - 29 * DAY, None),
            now
        ));
        assert!(!is_stale(&app("com.example", now - 90 * DAY, None), now));
    }
}
