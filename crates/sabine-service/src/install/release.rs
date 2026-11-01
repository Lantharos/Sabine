use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SystemUpdateMode {
    Required(SystemRequirement),
    Repair {
        required: SabineVersion,
        release_version: String,
    },
    Routine,
    Manual,
}

#[derive(Clone, Debug)]
pub struct StagedSystemUpdate {
    pub version: String,
    pub service: PathBuf,
    pub previous_service: Option<PathBuf>,
}

pub fn stage_system_update() -> ServiceResult<Option<StagedSystemUpdate>> {
    if read_installation_state().is_none() {
        return Ok(None);
    }
    install_latest_system(SystemUpdateMode::Routine, &mut |_| {})
}

pub(crate) fn stage_required_system_update(
    required: SystemRequirement,
) -> ServiceResult<Option<StagedSystemUpdate>> {
    install_latest_system(SystemUpdateMode::Required(required), &mut |_| {})
}

/// Installs the Sabine an update of `app` needs and restarts the daemon on it.
/// The daemon itself hands over through [`stage_required_system_update`].
pub(crate) fn install_required_system_update(
    required: SabineVersion,
    app: &str,
) -> ServiceResult<()> {
    let requirement = SystemRequirement {
        sabine: required,
        apps: vec![app.to_string()],
    };
    let Some(_) = stage_required_system_update(requirement)? else {
        return Err(ServiceError::Update(match held_system_update() {
            Some(held) => format!(
                "Sabine {} is waiting for {} to update",
                held.version,
                held.apps
                    .iter()
                    .map(|app| app.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => format!("Sabine {} is not available", required.label()),
        }));
    };
    crate::ensure_daemon_running()?;
    if installed_system_compatibility().accepts(required) {
        Ok(())
    } else {
        Err(ServiceError::Update(format!(
            "Sabine {} did not become active",
            required.label()
        )))
    }
}

pub fn repair_system_installation() -> ServiceResult<PathBuf> {
    let release_version = read_installation_state()
        .map(|state| state.active)
        .unwrap_or_else(|| crate::SABINE_VERSION.to_string());
    let required = SabineVersion::parse(&release_version).ok_or_else(|| {
        ServiceError::Update(format!(
            "installed Sabine version {release_version} is invalid"
        ))
    })?;
    install_latest_system(
        SystemUpdateMode::Repair {
            required,
            release_version,
        },
        &mut |_| {},
    )?
    .map(|update| update.service)
    .or_else(|| complete_service_at(cached_service_path()))
    .ok_or_else(|| ServiceError::Update("Sabine system repair did not produce a service".into()))
}

pub fn rollback_system_update(failed_version: &str) -> ServiceResult<Option<PathBuf>> {
    let _lock = lock_system_installation()?;
    let Some(state) = read_installation_state() else {
        return Ok(None);
    };
    if state.active != failed_version {
        return Ok(complete_managed_system_at(
            &versions_dir().join(&state.active),
        ));
    }
    let (Some(previous), Some(compatibility)) = (state.previous, state.previous_compatibility)
    else {
        return Ok(None);
    };
    let Some(service) = complete_managed_system_at(&versions_dir().join(&previous)) else {
        return Ok(None);
    };
    write_installation_state(&SystemInstallationState {
        schema: 1,
        active: previous,
        previous: None,
        compatibility,
        previous_compatibility: None,
    })?;
    record_system_failure(failed_version)?;
    let _ = sabine_runtime::remove_directory_if_present(&versions_dir().join(failed_version));
    Ok(Some(service))
}

pub(super) fn install_latest_system(
    mode: SystemUpdateMode,
    on_progress: &mut impl FnMut(PrepareProgress),
) -> ServiceResult<Option<StagedSystemUpdate>> {
    let requested_version = match &mode {
        SystemUpdateMode::Required(required) => Some(required.sabine.label()),
        SystemUpdateMode::Repair {
            release_version, ..
        } => Some(release_version.clone()),
        SystemUpdateMode::Routine | SystemUpdateMode::Manual => None,
    };
    let manifest = fetch_system_manifest(requested_version.as_deref())?;
    let manifest = if matches!(mode, SystemUpdateMode::Repair { .. }) {
        manifest
    } else {
        let Some(manifest) = installable_release(&mode, manifest)? else {
            return Ok(None);
        };
        manifest
    };
    install_system_release(mode, manifest, on_progress)
}

pub(super) fn install_system_release(
    mode: SystemUpdateMode,
    manifest: SystemReleaseManifest,
    on_progress: &mut impl FnMut(PrepareProgress),
) -> ServiceResult<Option<StagedSystemUpdate>> {
    let _lock = lock_system_installation()?;
    validate_system_release(&manifest)?;
    let compatibility = normalized_release_compatibility(&manifest)?;
    let required = match &mode {
        SystemUpdateMode::Required(required) => Some(required.sabine),
        SystemUpdateMode::Repair { required, .. } => Some(*required),
        SystemUpdateMode::Routine | SystemUpdateMode::Manual => None,
    };
    if let Some(required) = required
        && (compatibility.major != required.major || compatibility.build < required.build)
    {
        return Err(ServiceError::Update(format!(
            "latest Sabine system {} cannot run an app requiring Sabine {}",
            manifest.version,
            required.label()
        )));
    }
    if mode == SystemUpdateMode::Routine
        && (!crate::release_is_soaked(&manifest.published_at)
            || system_update_is_backed_off(&manifest.version))
    {
        return Ok(None);
    }
    let repairing = matches!(&mode, SystemUpdateMode::Repair { .. });
    let previous = read_installation_state();
    if !repairing
        && previous
            .as_ref()
            .is_some_and(|state| !crate::types::version_is_newer(&manifest.version, &state.active))
    {
        return Ok(None);
    }
    let install_dir = versions_dir().join(&manifest.version);
    let destination = install_dir.join(service_binary_name());
    if (repairing || complete_managed_system_at(&install_dir).is_none())
        && let Err(error) = install_system_archive(&manifest, &install_dir, on_progress)
    {
        record_system_failure(&manifest.version)?;
        return Err(error);
    }
    let (previous_version, previous_compatibility) = match previous {
        Some(state) if state.active == manifest.version => {
            (state.previous, state.previous_compatibility)
        }
        Some(state) => (Some(state.active), Some(state.compatibility)),
        None => (None, None),
    };
    write_installation_state(&SystemInstallationState {
        schema: 1,
        active: manifest.version.clone(),
        previous: previous_version.clone(),
        compatibility,
        previous_compatibility,
    })?;
    prune_system_versions(&manifest.version, previous_version.as_deref())?;
    Ok(Some(StagedSystemUpdate {
        version: manifest.version,
        service: destination,
        previous_service: previous_version
            .map(|version| versions_dir().join(version))
            .and_then(|directory| complete_managed_system_at(&directory)),
    }))
}

pub(crate) fn mark_system_update_healthy(version: &str) {
    let Ok(_lock) = lock_system_installation() else {
        return;
    };
    let _ = clear_system_failure(version);
}
