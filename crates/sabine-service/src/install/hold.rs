use super::*;
use crate::{HeldSystemUpdate, InstalledApp, SabineService, app::installed::installed_apps};

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
/// itself, or while it would stop installed apps from running or doesn't run
/// the app that requires it, the newest earlier release that does both. A
/// held release is recorded until the apps it waits for update or are removed.
pub(super) fn installable_release(
    mode: &SystemUpdateMode,
    latest: SystemReleaseManifest,
) -> ServiceResult<Option<SystemReleaseManifest>> {
    let candidate = normalized_release_compatibility(&latest)?;
    let current = installed_system_compatibility();
    let apps = installed_apps(&SabineService::default())?;
    let requirement = match mode {
        SystemUpdateMode::Required(requirement) => Some(requirement),
        _ => None,
    };
    let requiring = requirement.map_or(&[][..], |requirement| requirement.apps.as_slice());
    let blocking = blocking_apps(current, candidate, &apps, requiring);
    let runs_required = requirement.is_none_or(|requirement| candidate.accepts(requirement.sabine));
    if blocking.is_empty() && runs_required {
        if requirement.is_none() {
            clear_hold()?;
        }
        return Ok(Some(latest));
    }
    if !blocking.is_empty() {
        record_hold(&HeldSystemUpdate {
            version: latest.version,
            compatibility: candidate,
            apps: blocking,
        })?;
    }
    Ok(newest_compatible_release(
        current,
        candidate,
        &apps,
        requirement,
        |version| fetch_system_manifest(Some(&version.label())).ok(),
    ))
}

/// The apps `current` runs that a newer `candidate` would not, other than
/// `excluded`.
fn blocking_apps(
    current: SystemCompatibility,
    candidate: SystemCompatibility,
    apps: &[InstalledApp],
    excluded: &[String],
) -> Vec<InstalledApp> {
    let newer = (candidate.major, candidate.build) > (current.major, current.build);
    apps.iter()
        .filter(|app| {
            newer
                && current.accepts(app.sabine)
                && !candidate.accepts(app.sabine)
                && !excluded.contains(&app.id)
        })
        .cloned()
        .collect()
}

/// The newest release between `current` and `held` that runs every app and
/// what `requirement` asks for, fetching manifests newest first until one does.
fn newest_compatible_release(
    current: SystemCompatibility,
    held: SystemCompatibility,
    apps: &[InstalledApp],
    requirement: Option<&SystemRequirement>,
    fetch: impl Fn(SabineVersion) -> Option<SystemReleaseManifest>,
) -> Option<SystemReleaseManifest> {
    if held.major != current.major {
        return None;
    }
    let requiring = requirement.map_or(&[][..], |requirement| requirement.apps.as_slice());
    let mut candidates = (current.build + 1..held.build)
        .rev()
        .map(|build| SabineVersion {
            major: held.major,
            build,
        })
        .filter(|version| requirement.is_none_or(|requirement| *version >= requirement.sabine));
    candidates.find_map(|version| {
        let manifest = fetch(version)?;
        let compatibility = normalized_release_compatibility(&manifest).ok()?;
        (blocking_apps(current, compatibility, apps, requiring).is_empty()
            && requirement.is_none_or(|requirement| compatibility.accepts(requirement.sabine)))
        .then_some(manifest)
    })
}

fn record_hold(held: &HeldSystemUpdate) -> ServiceResult<()> {
    Ok(sabine_runtime::write_json_atomic(
        &held_update_path(),
        held,
    )?)
}

fn clear_hold() -> ServiceResult<()> {
    Ok(sabine_runtime::remove_file_if_present(&held_update_path())?)
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
    fn an_older_release_waits_for_nothing() {
        let apps = [app("current", 95), app("older", 91)];
        assert!(blocking_apps(system(95, 91), system(90, 66), &apps, &[]).is_empty());
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
        let chosen = newest_compatible_release(system(58, 53), system(62, 57), &apps, None, fetch);
        assert_eq!(chosen.map(|release| release.version), Some("0.60".into()));
        assert!(
            newest_compatible_release(system(60, 56), system(62, 57), &apps, None, fetch).is_none()
        );
    }

    #[test]
    fn a_required_update_installs_the_newest_release_that_runs_the_app_and_stops_no_other() {
        let apps = [app("notes", 56), app("tern", 55)];
        let releases = [release(59, 54), release(60, 56), release(61, 57)];
        let fetch = |version: SabineVersion| {
            releases
                .iter()
                .find(|release| release.compatibility.build == version.build)
                .cloned()
        };
        let requirement = |build| SystemRequirement {
            sabine: SabineVersion { major: 0, build },
            apps: vec!["tern".into()],
        };
        let chosen = newest_compatible_release(
            system(58, 53),
            system(62, 57),
            &apps,
            Some(&requirement(60)),
            fetch,
        );
        assert_eq!(chosen.map(|release| release.version), Some("0.60".into()));
        assert!(
            newest_compatible_release(
                system(58, 53),
                system(62, 57),
                &apps,
                Some(&requirement(63)),
                fetch
            )
            .is_none()
        );
    }
}
