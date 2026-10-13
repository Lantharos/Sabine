use super::*;
use state::{prune_system_versions_in, versions_dir_in, write_installation_state_in};

/// Copies the service, its daemon and the CEF host into `staging`, laid out
/// like an installed Sabine version.
pub(super) fn stage_system_files(staging: &Path, service: &Path, host: &Path) -> ServiceResult<()> {
    if staging.exists() {
        fs::remove_dir_all(staging)?;
    }
    fs::create_dir_all(staging)?;
    for source in [service.to_path_buf(), service_daemon_path(service)] {
        let name = source
            .file_name()
            .ok_or_else(|| ServiceError::Update(format!("{} is not a file", source.display())))?;
        if !source.is_file() {
            return Err(ServiceError::Update(format!(
                "Sabine system files are missing {}",
                source.display()
            )));
        }
        let target = staging.join(name);
        fs::copy(&source, &target)?;
        make_executable(&target)?;
    }
    if cfg!(target_os = "macos") {
        let bundle = host
            .ancestors()
            .find(|path| path.extension().is_some_and(|extension| extension == "app"))
            .ok_or_else(|| {
                ServiceError::Update("the Sabine host is not in an app bundle".into())
            })?;
        return copy_directory(bundle, &staging.join("sabine-host.app"));
    }
    if !sabine_host::host_is_complete(host) {
        return Err(ServiceError::Update(format!(
            "Sabine system files are missing {}",
            host.display()
        )));
    }
    let target = staging.join(sabine_host_relative_path());
    fs::copy(host, &target)?;
    make_executable(&target)?;
    if cfg!(windows) {
        fs::copy(host.with_extension("dll"), target.with_extension("dll"))?;
        fs::copy(
            host.with_file_name("chrome_elf.dll"),
            target.with_file_name("chrome_elf.dll"),
        )?;
    }
    Ok(())
}

/// Installs the service, daemon and CEF host for every user of the machine
/// into `root`, the system Sabine directory or its place in a staging tree.
/// Returns the installed version.
pub fn install_system_service(service: &Path, host: &Path, root: &Path) -> ServiceResult<String> {
    let version = crate::lifecycle::service_version(service)
        .ok_or_else(|| ServiceError::Update("the Sabine service has no version".into()))?;
    let versions = versions_dir_in(root);
    fs::create_dir_all(&versions)?;
    sabine_runtime::recover_directory_installs(&versions)?;
    let staging = versions.join(format!("{version}.installing"));
    stage_system_files(&staging, service, host)?;
    sabine_runtime::install_directory(&staging, &versions.join(&version))?;
    write_installation_state_in(
        root,
        &SystemInstallationState {
            schema: 1,
            compatibility: if version == crate::SABINE_VERSION {
                SystemCompatibility::current()
            } else {
                compatibility_for_version(&version)
            },
            active: version.clone(),
            previous: None,
            previous_compatibility: None,
        },
    )?;
    prune_system_versions_in(&versions, &version, None)?;
    Ok(version)
}
