use super::*;
use serde::Deserialize;

pub struct ComponentUpdate {
    _lock: sabine_runtime::FileLock,
    pub version: String,
    pub deferred: bool,
    pub system_updated: bool,
    pub cli: Option<PathBuf>,
}

pub fn update_components(
    force: bool,
    cli_version: &str,
    mut progress: impl FnMut(PrepareProgress),
) -> ServiceResult<ComponentUpdate> {
    let _lock = sabine_runtime::FileLock::acquire(
        &service_data_dir().join("manual-update.lock"),
        std::time::Duration::from_secs(600),
        |_| {},
    )?;
    let manifest = newest_release()?;
    validate_system_release(&manifest)?;
    let mut result = ComponentUpdate {
        _lock,
        version: manifest.version.clone(),
        deferred: false,
        system_updated: false,
        cli: None,
    };
    let update_cli = crate::types::version_is_newer(&manifest.version, cli_version);
    let update_system = installed_service_version()
        .is_none_or(|installed| crate::types::version_is_newer(&manifest.version, &installed));
    if !update_cli && !update_system {
        return Ok(result);
    }
    if !force && !crate::release_is_soaked(&manifest.published_at) {
        result.deferred = true;
        return Ok(result);
    }
    if update_system && system_update_is_backed_off(&manifest.version) {
        return Err(ServiceError::Update(format!(
            "Sabine {} previously failed its health check; use runtime doctor or repair before retrying",
            manifest.version
        )));
    }
    if update_cli {
        result.cli = Some(stage_cli(&manifest, &mut progress)?);
    }
    if update_system {
        install_system_release(SystemUpdateMode::Manual, manifest, &mut progress)?;
        crate::ensure_daemon_running()?;
        if installed_service_version().as_deref() != Some(&result.version) {
            return Err(ServiceError::Update(
                "Sabine update rolled back during startup".into(),
            ));
        }
        result.system_updated = true;
    }
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
    let releases: Vec<Release> = crate::http::fetch_manifest(&format!(
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

fn stage_cli(
    manifest: &SystemReleaseManifest,
    progress: &mut impl FnMut(PrepareProgress),
) -> ServiceResult<PathBuf> {
    let name = system_asset_name().replacen("sabine-system-", "sabine-cli-", 1);
    let artifact = manifest
        .artifacts
        .get(&name)
        .ok_or_else(|| ServiceError::Update(format!("Sabine release has no {name}")))?;
    if !artifact.url.starts_with("https://") {
        return Err(ServiceError::Update("CLI download must use HTTPS".into()));
    }
    let directory = service_data_dir()
        .join("downloads/cli")
        .join(&manifest.version);
    fs::create_dir_all(&directory)?;
    let archive = directory.join(&name);
    download_file(&artifact.url, &archive, artifact.size, progress)?;
    verify_sha256(&archive, &artifact.sha256)?;
    if fs::metadata(&archive)?.len() != artifact.size {
        return Err(ServiceError::Update("CLI archive size mismatch".into()));
    }
    let staging = directory.join("extracted");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    extract_system_archive(&archive, &staging)?;
    let binary = staging.join(if cfg!(windows) {
        "sabine.exe"
    } else {
        "sabine"
    });
    if !binary.is_file() {
        return Err(ServiceError::Update(
            "CLI archive has no sabine executable".into(),
        ));
    }
    make_executable(&binary)?;
    fs::remove_file(archive)?;
    Ok(binary)
}
