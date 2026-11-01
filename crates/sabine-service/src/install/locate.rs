use super::*;

pub fn installed_service_version() -> Option<String> {
    current_installation()
        .or_else(system_installation)
        .map(|(version, _)| version)
}

pub fn cached_service_path() -> PathBuf {
    current_installation()
        .map(|(_, path)| path.join(service_binary_name()))
        .unwrap_or_else(|| service_path_for_version(crate::SABINE_VERSION))
}

pub(crate) fn service_path_for_version(version: &str) -> PathBuf {
    versions_dir().join(version).join(service_binary_name())
}

/// Where a daemon of `version` runs from: the user's installation or the machine's.
pub(crate) fn daemon_paths_for_version(version: &str) -> [PathBuf; 2] {
    [
        service_path_for_version(version),
        state::versions_dir_in(&sabine_runtime::system_dir())
            .join(version)
            .join(service_binary_name()),
    ]
    .map(|service| service_daemon_path(&service))
}

pub fn service_daemon_path(service: &Path) -> PathBuf {
    service.with_file_name(service_daemon_binary_name())
}

/// `daemon` reached through the active-version link of the installation it
/// belongs to, when it is that installation's active daemon.
#[cfg(target_os = "linux")]
pub(crate) fn stable_daemon_path(daemon: &Path) -> Option<PathBuf> {
    let daemon = fs::canonicalize(daemon).ok()?;
    [
        sabine_runtime::sabine_data_dir(),
        sabine_runtime::system_dir(),
    ]
    .into_iter()
    .map(|root| state::active_dir_in(&root).join(service_daemon_binary_name()))
    .find(|stable| fs::canonicalize(stable).ok().as_ref() == Some(&daemon))
}

pub(super) fn complete_service_at(path: PathBuf) -> Option<PathBuf> {
    (path.is_file() && service_daemon_path(&path).is_file()).then_some(path)
}

pub(super) fn complete_managed_system_at(directory: &Path) -> Option<PathBuf> {
    sabine_host::host_is_complete(&directory.join(sabine_host_relative_path())).then_some(())?;
    complete_service_at(directory.join(service_binary_name()))
}

pub fn find_service_executable() -> Option<PathBuf> {
    if let Some(path) = configured_service().or_else(system_service) {
        return Some(path);
    }
    if let Some((_, directory)) = current_installation() {
        return complete_service_at(directory.join(service_binary_name()));
    }
    adjacent_service().or_else(service_on_path)
}

pub fn ensure_service_executable(
    mut on_progress: impl FnMut(PrepareProgress),
) -> ServiceResult<PathBuf> {
    if let Some(path) = configured_service().or_else(system_service) {
        return Ok(path);
    }
    let mut current = current_installation();
    if current.is_none() && read_installation_state().is_some() {
        let _lock = lock_system_installation()?;
        current = current_installation();
    }
    if let Some((version, directory)) = current {
        let current = directory.join(service_binary_name());
        if !managed_system_is_older(&version) {
            return Ok(current);
        }
        report(&mut on_progress, "Updating Sabine service");
        return Ok(install_required_service(&mut on_progress)?.unwrap_or(current));
    }
    if let Some(state) = read_installation_state() {
        report(&mut on_progress, "Repairing Sabine service");
        if let Some(service) = repair_service(state.active, &mut on_progress)? {
            return Ok(service);
        }
    }
    if let Some(path) = adjacent_service() {
        return seed_managed_install(&path);
    }
    if let Some(path) = service_on_path() {
        return Ok(path);
    }
    report(&mut on_progress, "Downloading Sabine service");
    let destination = install_latest_system(
        SystemUpdateMode::Required(this_app_requirement()),
        &mut on_progress,
    )?
    .map(|update| update.service)
    .or_else(|| complete_service_at(cached_service_path()))
    .ok_or_else(|| ServiceError::Update("Sabine system installation is incomplete".into()))?;
    on_progress(PrepareProgress {
        stage: PrepareStage::Service,
        message: "Sabine service ready".to_string(),
        fraction: Some(0.08),
    });
    Ok(destination)
}

/// The service an app beside its own Sabine installs, or else the newest
/// release that runs this app.
fn install_required_service(
    on_progress: &mut impl FnMut(PrepareProgress),
) -> ServiceResult<Option<PathBuf>> {
    if let Some(path) = adjacent_service() {
        return seed_managed_install(&path).map(Some);
    }
    Ok(install_latest_system(
        SystemUpdateMode::Required(this_app_requirement()),
        on_progress,
    )?
    .map(|update| update.service))
}

/// Restores the recorded installation of version `active` when its files are
/// incomplete.
fn repair_service(
    active: String,
    on_progress: &mut impl FnMut(PrepareProgress),
) -> ServiceResult<Option<PathBuf>> {
    if managed_system_is_older(&active) {
        return install_required_service(on_progress);
    }
    let installed = SabineVersion::parse(&active).ok_or_else(|| {
        ServiceError::Update(format!("installed Sabine version {active} is invalid"))
    })?;
    if installed == SabineVersion::current()
        && let Some(path) = adjacent_service()
    {
        return seed_managed_install(&path).map(Some);
    }
    Ok(install_latest_system(
        SystemUpdateMode::Repair {
            required: installed,
            release_version: active,
        },
        on_progress,
    )?
    .map(|update| update.service))
}

fn report(on_progress: &mut impl FnMut(PrepareProgress), message: &str) {
    on_progress(PrepareProgress {
        stage: PrepareStage::Service,
        message: message.to_string(),
        fraction: Some(0.02),
    });
}

fn configured_service() -> Option<PathBuf> {
    std::env::var_os("SABINE_SERVICE_PATH")
        .map(PathBuf::from)
        .and_then(complete_service_at)
}

/// The machine's shared service, unless it is older than this app needs or
/// than the user's own installation.
fn system_service() -> Option<PathBuf> {
    let (version, directory) = system_installation()?;
    let superseded = current_installation()
        .is_some_and(|(user, _)| crate::types::version_is_newer(&user, &version));
    (!managed_system_is_older(&version) && !superseded)
        .then(|| directory.join(service_binary_name()))
}

fn adjacent_service() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    complete_service_at(current.parent()?.join(service_binary_name()))
}

fn service_on_path() -> Option<PathBuf> {
    sabine_runtime::find_program(service_binary_name()).and_then(complete_service_at)
}
