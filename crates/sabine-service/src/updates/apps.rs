use std::path::{Path, PathBuf};

use crate::app::registry::{RegistryLock, SabineService};
use crate::download::{download_artifact, verify_sha256};
use crate::types::{
    AppArtifact, AppArtifactKind, AppInstallMode, AppReleaseManifest, AppUpdateStatus,
    PendingAppUpdate, SabineVersion, ServiceError, ServiceResult, is_https_url, unix_timestamp,
    update_artifact_target, version_is_newer,
};
use crate::{release_is_soaked, verify_app_release};

use super::installers::{self, artifact_extension, install_archive, pending_path};
use super::packages::run_package_installer;

/// What an app update does when it needs a newer Sabine than the installed one.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SystemUpdates {
    /// Install it and restart the daemon on it, outside the daemon.
    Install,
    /// Report it, for the daemon to hand over to.
    Report,
}

/// Whether an archive update installs once it's downloaded, or waits for the
/// person to accept it the next time the app opens, as packages always do.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArchiveUpdates {
    Install,
    AskFirst,
}

impl SabineService {
    pub fn update_app(&self, id: &str) -> ServiceResult<AppUpdateStatus> {
        self.update_app_with_soak(id, false)
    }

    /// Updates `id`, after its soak period when `require_soak`. Without it the
    /// update also installs the Sabine it needs.
    pub fn update_app_with_soak(
        &self,
        id: &str,
        require_soak: bool,
    ) -> ServiceResult<AppUpdateStatus> {
        let system = if require_soak {
            SystemUpdates::Report
        } else {
            SystemUpdates::Install
        };
        self.update_app_as(id, require_soak, system, ArchiveUpdates::Install)
    }

    pub(super) fn update_app_as(
        &self,
        id: &str,
        require_soak: bool,
        system_updates: SystemUpdates,
        archive_updates: ArchiveUpdates,
    ) -> ServiceResult<AppUpdateStatus> {
        let _update_lock = self.app_update_lock(id)?;
        let app = self.app(id)?;
        let update = app
            .manifest
            .update
            .as_ref()
            .ok_or_else(|| ServiceError::Update("app has no update source".to_string()))?;
        if update.install_mode == AppInstallMode::Store {
            return Ok(AppUpdateStatus::StoreManaged);
        }
        let manifest_url = update.source.manifest_url(&update.channel)?;
        let release = fetch_release(&manifest_url)?;
        verify_app_release(&release, &update.public_key)?;
        validate_release(&release, id, &update.channel)?;
        if !version_is_newer(&release.version, &app.manifest.version) {
            return Ok(AppUpdateStatus::Current);
        }
        if require_soak && !release_is_soaked(&release.published_at) {
            return Ok(AppUpdateStatus::Deferred {
                version: release.version,
            });
        }
        let system = crate::install::installed_system_compatibility();
        let installed = SabineVersion {
            major: system.major,
            build: system.build,
        };
        if release.requires_sabine > installed {
            if system_updates == SystemUpdates::Report {
                return Ok(AppUpdateStatus::RequiresSystem {
                    app_version: release.version,
                    sabine: release.requires_sabine,
                });
            }
            crate::install::install_required_system_update(release.requires_sabine, id)?;
        }
        let system = crate::install::installed_system_compatibility();
        if !system.accepts(release.requires_sabine) {
            return Err(ServiceError::Update(format!(
                "app update {} requires Sabine {}, installed system is {}.{}",
                release.version,
                release.requires_sabine.label(),
                system.major,
                system.build
            )));
        }
        let target = update_artifact_target(update.install_mode, update.package_kind);
        let artifact = release
            .artifacts
            .get(&target)
            .ok_or_else(|| ServiceError::Update(format!("release has no artifact for {target}")))?;
        validate_artifact(artifact)?;
        let executable = if update.install_mode == AppInstallMode::Managed
            && artifact.kind == AppArtifactKind::Archive
        {
            Some(artifact.executable.clone().ok_or_else(|| {
                ServiceError::Update("managed update artifact has no executable".to_string())
            })?)
        } else {
            None
        };
        let executable = match executable {
            Some(executable) if archive_updates == ArchiveUpdates::Install => executable,
            executable => {
                let pending = self.stage_update(id, &release, artifact, executable)?;
                return Ok(AppUpdateStatus::PendingApproval(pending));
            }
        };
        let download = self.download_path(id, &release.version, artifact.kind);
        self.install_managed_release(
            id,
            &release.version,
            release.requires_sabine,
            &executable,
            || {
                self.download_verified(&download, artifact)?;
                Ok(download.clone())
            },
        )?;
        sabine_runtime::remove_file_if_present(&download)?;
        Ok(AppUpdateStatus::Installed {
            version: release.version,
        })
    }

    pub fn pending_app_update(&self, id: &str) -> ServiceResult<Option<PendingAppUpdate>> {
        validate_app_id(id)?;
        let path = pending_path(&self.root, id);
        if !path.is_file() {
            return Ok(None);
        }
        let pending: PendingAppUpdate =
            serde_json::from_slice(&std::fs::read(&path)?).map_err(|error| {
                ServiceError::Decode {
                    path,
                    source: error,
                }
            })?;
        let app = self.app(id)?;
        Ok(version_is_newer(&pending.version, &app.manifest.version).then_some(pending))
    }

    pub fn apply_pending_app_update(
        &self,
        id: &str,
        install_target: Option<&Path>,
    ) -> ServiceResult<bool> {
        let _update_lock = self.app_update_lock(id)?;
        let Some(pending) = self.pending_app_update(id)? else {
            return Ok(false);
        };
        verify_sha256(&pending.artifact, &pending.sha256)?;
        match &pending.executable {
            Some(executable) => self.install_managed_release(
                id,
                &pending.version,
                pending.sabine,
                executable,
                || Ok(pending.artifact.clone()),
            )?,
            None => run_package_installer(&pending, install_target)?,
        }
        std::fs::remove_file(pending_path(&self.root, id))?;
        sabine_runtime::remove_file_if_present(&pending.artifact)?;
        Ok(true)
    }

    pub fn defer_pending_app_update(&self, id: &str) -> ServiceResult<bool> {
        let _update_lock = self.app_update_lock(id)?;
        let Some(mut pending) = self.pending_app_update(id)? else {
            return Ok(false);
        };
        pending.prompt_after = unix_timestamp().saturating_add(crate::UPDATE_SOAK.as_secs());
        sabine_runtime::write_json_atomic(&pending_path(&self.root, id), &pending)?;
        Ok(true)
    }

    pub(crate) fn app_update_lock(&self, id: &str) -> ServiceResult<sabine_runtime::FileLock> {
        validate_app_id(id)?;
        Ok(sabine_runtime::FileLock::acquire(
            &self.root.join("apps").join(id).join("update.lock"),
            std::time::Duration::from_secs(10),
            |_| {},
        )?)
    }

    fn download_path(&self, id: &str, version: &str, kind: AppArtifactKind) -> PathBuf {
        self.root
            .join("downloads")
            .join(id)
            .join(format!("{version}.{}", artifact_extension(kind)))
    }

    /// Downloads `artifact` to `path`, keeping a copy already there that
    /// matches its checksum.
    fn download_verified(&self, path: &Path, artifact: &AppArtifact) -> ServiceResult<()> {
        std::fs::create_dir_all(path.parent().expect("downloads have a folder"))?;
        if path.is_file() && verify_sha256(path, &artifact.sha256).is_err() {
            std::fs::remove_file(path)?;
        }
        if !path.is_file() {
            download_artifact(&artifact.url, path, None, |_| {})?;
        }
        verify_sha256(path, &artifact.sha256)
    }

    fn stage_update(
        &self,
        id: &str,
        release: &AppReleaseManifest,
        artifact: &AppArtifact,
        executable: Option<PathBuf>,
    ) -> ServiceResult<PendingAppUpdate> {
        let path = self.download_path(id, &release.version, artifact.kind);
        self.download_verified(&path, artifact)?;
        let existing = self
            .pending_app_update(id)?
            .filter(|pending| pending.version == release.version);
        let pending = PendingAppUpdate {
            app_id: id.to_string(),
            version: release.version.clone(),
            artifact: path,
            sha256: artifact.sha256.clone(),
            kind: artifact.kind,
            requires_elevation: artifact.kind.requires_elevation(),
            staged_at: existing
                .as_ref()
                .map(|pending| pending.staged_at)
                .filter(|timestamp| *timestamp > 0)
                .unwrap_or_else(unix_timestamp),
            prompt_after: existing
                .as_ref()
                .map(|pending| pending.prompt_after)
                .unwrap_or_default(),
            executable,
            sabine: release.requires_sabine,
        };
        sabine_runtime::write_json_atomic(&pending_path(&self.root, id), &pending)?;
        Ok(pending)
    }

    /// Installs an archive release beside the current version and makes it
    /// the one the app launches.
    fn install_managed_release(
        &self,
        id: &str,
        version: &str,
        sabine: SabineVersion,
        executable: &Path,
        archive: impl FnOnce() -> ServiceResult<PathBuf>,
    ) -> ServiceResult<()> {
        let release_dir = self
            .root
            .join("apps")
            .join(id)
            .join("releases")
            .join(version);
        if let Some(parent) = release_dir.parent() {
            sabine_runtime::recover_directory_installs(parent)?;
        }
        let installed_executable = release_dir.join(executable);
        if installers::validate_executable(&installed_executable).is_err() {
            install_archive(&archive()?, executable, &release_dir)?;
        }
        installers::validate_executable(&installed_executable)?;
        self.activate_managed_update(id, version, sabine, installed_executable)
    }

    fn activate_managed_update(
        &self,
        id: &str,
        version: &str,
        sabine: SabineVersion,
        executable: PathBuf,
    ) -> ServiceResult<()> {
        let _lock = RegistryLock::acquire(&self.root)?;
        let mut registry = self.load_registry()?;
        let registered = registry
            .apps
            .get_mut(id)
            .ok_or_else(|| ServiceError::AppNotFound(id.to_string()))?;
        if !version_is_newer(version, &registered.manifest.version) {
            return Ok(());
        }
        registered.manifest.version = version.to_string();
        registered.manifest.sabine = sabine;
        registered.manifest.executable = executable;
        registered.updated_at = unix_timestamp();
        self.save_registry(&registry)
    }
}

fn validate_app_id(id: &str) -> ServiceResult<()> {
    if crate::valid_app_id(id) {
        Ok(())
    } else {
        Err(ServiceError::Update("invalid app identifier".to_string()))
    }
}

fn fetch_release(url: &str) -> ServiceResult<AppReleaseManifest> {
    if !is_https_url(url) {
        return Err(ServiceError::Update(
            "release manifest URL must use HTTPS".to_string(),
        ));
    }
    crate::download::fetch_manifest(url)
}

fn validate_release(release: &AppReleaseManifest, id: &str, channel: &str) -> ServiceResult<()> {
    if release.schema != 1 {
        return Err(ServiceError::Update(format!(
            "unsupported release manifest schema {}",
            release.schema
        )));
    }
    if release.app_id != id {
        return Err(ServiceError::Update(format!(
            "release belongs to {} instead of {id}",
            release.app_id
        )));
    }
    if release.channel != channel {
        return Err(ServiceError::Update(format!(
            "release channel {} does not match {channel}",
            release.channel
        )));
    }
    Ok(())
}

fn validate_artifact(artifact: &AppArtifact) -> ServiceResult<()> {
    if !is_https_url(&artifact.url) {
        return Err(ServiceError::Update(
            "artifact URL must use HTTPS".to_string(),
        ));
    }
    if artifact.sha256.len() != 64 || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(ServiceError::Update(
            "artifact SHA-256 is invalid".to_string(),
        ));
    }
    if artifact
        .executable
        .as_deref()
        .is_some_and(|path| !installers::safe_relative_path(path))
    {
        return Err(ServiceError::Update(
            "artifact executable path is unsafe".to_string(),
        ));
    }
    Ok(())
}
