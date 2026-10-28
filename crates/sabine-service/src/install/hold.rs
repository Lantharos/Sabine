use super::*;
use crate::{
    HeldSystemUpdate, InstalledApp, SabineService, app::installed::installed_apps,
    app::registry::replace_file,
};

fn held_update_path() -> PathBuf {
    sabine_runtime::sabine_data_dir().join("bin/held-update.json")
}

/// The Sabine release that is waiting because it would stop installed apps
/// from running, with the apps it still waits for.
pub fn held_system_update() -> Option<HeldSystemUpdate> {
    let mut held =
        serde_json::from_slice::<HeldSystemUpdate>(&fs::read(held_update_path()).ok()?).ok()?;
    let apps = installed_apps(&SabineService::default()).ok()?;
    held.apps = blocking_apps(
        installed_system_compatibility(),
        held.compatibility,
        &apps,
        &[],
    );
    (!held.apps.is_empty()).then_some(held)
}

/// The release to install for `mode` when `latest` is the newest one: `latest`
/// itself, or while it would stop installed apps from running, the newest
/// earlier release that runs them all. A held release is recorded until the
/// apps it waits for update or are removed.
pub(super) fn installable_release(
    mode: &SystemUpdateMode,
    latest: SystemReleaseManifest,
) -> ServiceResult<Option<SystemReleaseManifest>> {
    validate_system_release(&latest)?;
    let candidate = normalized_release_compatibility(&latest)?;
    let current = installed_system_compatibility();
    let apps = installed_apps(&SabineService::default())?;
    let requiring = match mode {
        SystemUpdateMode::Required(requirement) => requirement.apps.as_slice(),
        _ => &[],
    };
    let blocking = blocking_apps(current, candidate, &apps, requiring);
    if blocking.is_empty() {
        if matches!(mode, SystemUpdateMode::Routine | SystemUpdateMode::Manual) {
            clear_hold()?;
        }
        return Ok(Some(latest));
    }
    record_hold(&HeldSystemUpdate {
        version: latest.version,
        compatibility: candidate,
        apps: blocking,
    })?;
    if matches!(mode, SystemUpdateMode::Required(_)) {
        return Ok(None);
    }
    Ok(newest_compatible_release(
        current,
        candidate,
        &apps,
        |version| {
            fetch_system_manifest(Some(&version.label()))
                .ok()
                .filter(|manifest| validate_system_release(manifest).is_ok())
        },
    ))
}

/// The apps `current` runs that `candidate` would not, other than `excluded`.
fn blocking_apps(
    current: SystemCompatibility,
    candidate: SystemCompatibility,
    apps: &[InstalledApp],
    excluded: &[String],
) -> Vec<InstalledApp> {
    apps.iter()
        .filter(|app| {
            current.accepts(app.sabine)
                && !candidate.accepts(app.sabine)
                && !excluded.contains(&app.id)
        })
        .cloned()
        .collect()
}

fn newest_compatible_release(
    current: SystemCompatibility,
    held: SystemCompatibility,
    apps: &[InstalledApp],
    fetch: impl Fn(SabineVersion) -> Option<SystemReleaseManifest>,
) -> Option<SystemReleaseManifest> {
    if held.major != current.major {
        return None;
    }
    (current.build + 1..held.build).rev().find_map(|build| {
        let manifest = fetch(SabineVersion {
            major: held.major,
            build,
        })?;
        let compatibility = normalized_release_compatibility(&manifest).ok()?;
        blocking_apps(current, compatibility, apps, &[])
            .is_empty()
            .then_some(manifest)
    })
}

fn record_hold(held: &HeldSystemUpdate) -> ServiceResult<()> {
    let path = held_update_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("new");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(held).expect("a held update is serializable"),
    )?;
    replace_file(&temporary, &path)?;
    Ok(())
}

fn clear_hold() -> ServiceResult<()> {
    match fs::remove_file(held_update_path()) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system(build: u32, minimum_app_build: u32) -> SystemCompatibility {
        SystemCompatibility {
            major: 0,
            build,
            minimum_app_build,
        }
    }

    fn app(id: &str, build: u32) -> InstalledApp {
        InstalledApp {
            id: id.to_string(),
            name: id.to_string(),
            sabine: SabineVersion { major: 0, build },
            system: false,
        }
    }

    fn release(build: u32, minimum_app_build: u32) -> SystemReleaseManifest {
        SystemReleaseManifest {
            schema: 1,
            version: format!("0.{build}"),
            published_at: "0".to_string(),
            compatibility: system(build, minimum_app_build),
            artifacts: Default::default(),
            signature: String::new(),
        }
    }

    #[test]
    fn only_apps_the_update_would_stop_block_it() {
        let apps = [
            app("old", 55),
            app("retired", 50),
            app("newer", 61),
            app("current", 58),
        ];
        let blocking = blocking_apps(system(58, 53), system(60, 57), &apps, &[]);
        assert_eq!(blocking, [app("old", 55)]);
        assert!(blocking_apps(system(58, 53), system(60, 57), &apps, &["old".into()]).is_empty());
    }

    #[test]
    fn a_held_update_falls_back_to_the_newest_release_that_runs_every_app() {
        let apps = [app("notes", 56)];
        let releases = [release(59, 54), release(60, 56), release(61, 57)];
        let fetch = |version: SabineVersion| {
            releases
                .iter()
                .find(|release| release.compatibility.build == version.build)
                .cloned()
        };
        let chosen = newest_compatible_release(system(58, 53), system(62, 57), &apps, fetch);
        assert_eq!(chosen.map(|release| release.version), Some("0.60".into()));
        assert!(newest_compatible_release(system(60, 56), system(62, 57), &apps, fetch).is_none());
    }
}
