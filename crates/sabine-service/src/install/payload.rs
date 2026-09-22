use super::*;

pub(super) fn seed_managed_install(service: &Path) -> ServiceResult<PathBuf> {
    let _lock = lock_system_installation()?;
    if let Some((version, directory)) = current_installation()
        && !managed_system_is_older(&version)
    {
        return Ok(directory.join(service_binary_name()));
    }
    let source_dir = service.parent().ok_or_else(|| {
        ServiceError::Update("bundled Sabine service has no parent directory".to_string())
    })?;
    let version = crate::SABINE_VERSION;
    let destination = versions_dir().join(version);
    let installed_service = destination.join(service_binary_name());
    if complete_service_at(installed_service.clone()).is_none()
        || !sabine_host::host_is_complete(&destination.join(sabine_host_relative_path()))
    {
        let staging = versions_dir().join(format!("{version}.installing"));
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;
        for name in [service_binary_name(), service_daemon_binary_name()] {
            let source = source_dir.join(name);
            if !source.is_file() {
                return Err(ServiceError::Update(format!(
                    "offline Sabine system bundle is missing {name}"
                )));
            }
            let target = staging.join(name);
            fs::copy(source, &target)?;
            make_executable(&target)?;
        }
        if cfg!(target_os = "macos") {
            copy_directory(
                &source_dir.join("sabine-host.app"),
                &staging.join("sabine-host.app"),
            )?;
        } else {
            let name = sabine_host_relative_path();
            let source = source_dir.join(&name);
            if !source.is_file() {
                return Err(ServiceError::Update(format!(
                    "offline Sabine system bundle is missing {}",
                    name.display()
                )));
            }
            let target = staging.join(name);
            fs::copy(&source, &target)?;
            make_executable(&target)?;
            if cfg!(windows) {
                fs::copy(source.with_extension("dll"), target.with_extension("dll"))?;
                fs::copy(
                    source.with_file_name("chrome_elf.dll"),
                    target.with_file_name("chrome_elf.dll"),
                )?;
            }
        }
        sabine_runtime::install_directory(&staging, &destination)?;
    }
    let previous_state = read_installation_state();
    let previous = previous_state
        .as_ref()
        .map(|state| state.active.clone())
        .filter(|active| active != version);
    let previous_compatibility = previous_state
        .as_ref()
        .filter(|state| state.active != version)
        .map(normalized_state_compatibility);
    write_installation_state(&SystemInstallationState {
        schema: 1,
        active: version.to_string(),
        previous,
        compatibility: SystemCompatibility::current(),
        previous_compatibility,
    })?;
    Ok(installed_service)
}

pub(super) fn install_system_archive(
    manifest: &SystemReleaseManifest,
    install_dir: &Path,
    on_progress: &mut impl FnMut(PrepareProgress),
) -> ServiceResult<()> {
    let name = system_asset_name();
    let artifact = manifest
        .artifacts
        .get(&name)
        .ok_or_else(|| ServiceError::Update(format!("Sabine release has no {name} artifact")))?;
    if !artifact.url.starts_with("https://") {
        return Err(ServiceError::Update(
            "Sabine system artifact URL must use HTTPS".to_string(),
        ));
    }
    if artifact.sha256.len() != 64 || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(ServiceError::Update(
            "Sabine system artifact SHA-256 is invalid".to_string(),
        ));
    }
    let downloads = service_data_dir().join("downloads/system");
    fs::create_dir_all(&downloads)?;
    let archive = downloads.join(format!("{}-{name}", manifest.version));
    download_file(&artifact.url, &archive, artifact.size, on_progress)?;
    verify_sha256(&archive, &artifact.sha256)?;
    let actual_size = fs::metadata(&archive)?.len();
    if actual_size != artifact.size {
        return Err(ServiceError::Update(format!(
            "Sabine system bundle size mismatch: expected {}, got {actual_size}",
            artifact.size
        )));
    }

    let staging = versions_dir().join(format!("{}.installing", manifest.version));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    extract_system_archive(&archive, &staging)?;
    for name in [
        PathBuf::from(service_binary_name()),
        PathBuf::from(service_daemon_binary_name()),
        sabine_host_relative_path(),
    ] {
        let source = staging.join(&name);
        if !source.is_file() {
            return Err(ServiceError::Update(format!(
                "Sabine system bundle is missing {}",
                name.display()
            )));
        }
        make_executable(&source)?;
    }
    if !sabine_host::host_is_complete(&staging.join(sabine_host_relative_path())) {
        return Err(ServiceError::Update(
            "Sabine system bundle has an incomplete native host".into(),
        ));
    }
    sabine_runtime::install_directory(&staging, install_dir)?;
    let _ = fs::remove_file(archive);
    Ok(())
}
