use super::*;
use sabine_runtime::sabine_data_dir;
use serde::Deserialize;

pub struct ComponentUpdate {
    _lock: sabine_runtime::FileLock,
    pub version: String,
    pub deferred: bool,
    pub system: Option<String>,
    pub held: Option<crate::HeldSystemUpdate>,
}

pub fn update_components(
    force: bool,
    mut progress: impl FnMut(PrepareProgress),
) -> ServiceResult<ComponentUpdate> {
    let _lock = sabine_runtime::FileLock::acquire(
        &sabine_data_dir().join("manual-update.lock"),
        std::time::Duration::from_secs(600),
        |_| {},
    )?;
    let manifest = newest_release()?;
    validate_system_release(&manifest)?;
    let mut result = ComponentUpdate {
        _lock,
        version: manifest.version.clone(),
        deferred: false,
        system: None,
        held: None,
    };
    let update_system = installed_service_version()
        .is_none_or(|installed| crate::types::version_is_newer(&manifest.version, &installed));
    if !update_system {
        return Ok(result);
    }
    if !force && !crate::release_is_soaked(&manifest.published_at) {
        result.deferred = true;
        return Ok(result);
    }
    if system_update_is_backed_off(&manifest.version) {
        return Err(ServiceError::Update(format!(
            "Sabine {} previously failed its health check; use runtime doctor or repair before retrying",
            manifest.version
        )));
    }
    if let Some(release) = installable_release(&SystemUpdateMode::Manual, manifest)? {
        let version = release.version.clone();
        install_system_release(SystemUpdateMode::Manual, release, &mut progress)?;
        crate::ensure_daemon_running()?;
        if installed_service_version().as_deref() != Some(&version) {
            return Err(ServiceError::Update(
                "Sabine update rolled back during startup".into(),
            ));
        }
        result.system = Some(version);
    }
    result.held = held_system_update();
    Ok(result)
}

fn newest_release() -> ServiceResult<SystemReleaseManifest> {
    if std::env::var_os("SABINE_RELEASE_MANIFEST_URL").is_some() {
        return fetch_system_manifest(None);
    }
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        draft: bool,
        prerelease: bool,
    }
    let releases: Vec<Release> = crate::download::fetch_manifest(&format!(
        "https://api.github.com/repos/{SERVICE_REPO}/releases?per_page=100"
    ))?;
    let version = releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| SabineVersion::parse(release.tag_name.trim_start_matches('v')))
        .max()
        .ok_or_else(|| ServiceError::Update("no published stable Sabine release found".into()))?;
    fetch_system_manifest(Some(&version.label()))
}
