use crate::{
    PrepareProgress, PrepareStage, SabineVersion, ServiceError, ServiceResult, SystemCompatibility,
    SystemReleaseManifest, SystemRequirement, verify_system_release,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::download::verify_sha256;

mod artifacts;
mod hold;
mod locate;
mod manual;
mod payload;
mod release;
mod state;
mod system;
pub use hold::held_system_update;
use hold::installable_release;
#[cfg(target_os = "linux")]
pub(crate) use locate::stable_daemon_path;
pub use locate::{
    cached_service_path, ensure_service_executable, find_service_executable,
    installed_service_version, service_daemon_path,
};
use locate::{complete_managed_system_at, complete_service_at};
pub(crate) use locate::{daemon_paths_for_version, service_path_for_version};
pub use manual::update_components;
use payload::{install_system_archive, seed_managed_install};
pub(crate) use release::{
    StagedSystemUpdate, install_required_system_update, mark_system_update_healthy,
    repair_system_installation, rollback_system_update, stage_required_system_update,
    stage_system_update,
};
use release::{SystemUpdateMode, install_latest_system, install_system_release};
pub use system::{install_system_service, stage_system_files};

use artifacts::{
    download_system_artifact, fetch_system_manifest, sabine_host_relative_path,
    service_binary_name, service_daemon_binary_name, system_asset_name,
};
use state::{
    SystemInstallationState, clear_system_failure, compatibility_for_version, current_installation,
    lock_system_installation, prune_system_versions, read_installation_state,
    read_installation_state_in, record_system_failure, system_installation,
    system_update_is_backed_off, versions_dir, write_installation_state,
};

const SERVICE_REPO: &str = "Lantharos/Sabine";
const SYSTEM_UPDATE_PUBLIC_KEYS: &str = include_str!("../update-public-key.txt");

pub fn installed_system_compatibility() -> SystemCompatibility {
    system_compatibility_in(&sabine_runtime::sabine_data_dir())
}

/// What the newest Sabine accepts, of the one installed under `root` and the
/// one installed for every user.
pub(crate) fn system_compatibility_in(root: &Path) -> SystemCompatibility {
    [root, &sabine_runtime::system_dir()]
        .into_iter()
        .filter_map(read_installation_state_in)
        .map(|state| state.compatibility)
        .max_by_key(|compatibility| (compatibility.major, compatibility.build))
        .unwrap_or_else(SystemCompatibility::current)
}

fn this_app_requirement() -> SystemRequirement {
    SystemRequirement {
        sabine: SabineVersion::current(),
        apps: Vec::new(),
    }
}

fn managed_system_is_older(installed: &str) -> bool {
    crate::types::version_is_newer(crate::SABINE_VERSION, installed)
}

fn normalized_release_compatibility(
    manifest: &SystemReleaseManifest,
) -> ServiceResult<SystemCompatibility> {
    let version = SabineVersion::parse(&manifest.version).ok_or_else(|| {
        ServiceError::Update(format!(
            "invalid Sabine release version {}",
            manifest.version
        ))
    })?;
    let compatibility = manifest.compatibility;
    if compatibility.major != version.major
        || compatibility.build != version.build
        || compatibility.minimum_app_build > compatibility.build
    {
        return Err(ServiceError::Update(
            "Sabine release compatibility is inconsistent with its version".to_string(),
        ));
    }
    Ok(compatibility)
}

fn validate_system_release(manifest: &SystemReleaseManifest) -> ServiceResult<()> {
    if !SYSTEM_UPDATE_PUBLIC_KEYS
        .lines()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .any(|key| verify_system_release(manifest, key).is_ok())
    {
        return Err(ServiceError::Update(
            "Sabine release signature is not trusted".to_string(),
        ));
    }
    if manifest.schema != 1 {
        return Err(ServiceError::Update(format!(
            "unsupported Sabine release schema {}",
            manifest.schema
        )));
    }
    if manifest.version.trim().is_empty() {
        return Err(ServiceError::Update(
            "Sabine release version is missing".to_string(),
        ));
    }
    normalized_release_compatibility(manifest)?;
    Ok(())
}

/// An empty folder at `staging`, replacing whatever an interrupted attempt left.
fn empty_staging(staging: &Path) -> ServiceResult<()> {
    sabine_runtime::remove_directory_if_present(staging)?;
    fs::create_dir_all(staging)?;
    Ok(())
}
