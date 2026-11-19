use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use crate::download::verify_sha256;
use crate::types::{
    AppInstallMode, AppUpdateConfig, AppUpdateSource, PendingAppUpdate, ServiceError,
    ServiceResult, UPDATE_SOAK, default_maintenance_interval, unix_timestamp,
    update_artifact_target, version_is_newer,
};
use crate::{release_is_soaked, verify_app_release};

use super::apps::{download_verified, fetch_release, validate_artifact, validate_release};
use super::installers::artifact_extension;
use super::packages::run_package_installer;

/// The updates of an app that carries its own Chromium and never registers
/// with Sabine. They live in the app's cache folder, so no Sabine on the
/// computer ever touches them.
pub struct StandaloneUpdates {
    root: PathBuf,
    id: String,
    version: String,
    update: AppUpdateConfig,
}

impl StandaloneUpdates {
    /// The updates of a standalone app installed from one of its packages.
    /// An archive has no installer to update it with, and a store updates
    /// the apps it installed itself.
    pub fn new(id: &str, version: &str, update: &AppUpdateConfig) -> Option<Self> {
        if update.install_mode != AppInstallMode::Package
            || update.source == AppUpdateSource::Schelf
        {
            return None;
        }
        Some(Self {
            root: dirs::cache_dir()?.join(id).join("updates"),
            id: id.to_string(),
            version: version.to_string(),
            update: update.clone(),
        })
    }

    pub fn pending_path(&self) -> PathBuf {
        self.root.join("pending-update.json")
    }

    /// Downloads a newer release once it has been out for a day, asking the
    /// update source at most once per maintenance interval.
    pub fn check(&self) -> ServiceResult<()> {
        let _lock = sabine_runtime::FileLock::acquire(
            &self.root.join("update.lock"),
            Duration::from_secs(10),
            |_| {},
        )?;
        let checked = self.root.join("checked");
        if modified_within(&checked, default_maintenance_interval()) {
            return Ok(());
        }
        let release = fetch_release(
            &self
                .update
                .source
                .manifest_url(&self.id, &self.update.channel)?,
        )?;
        verify_app_release(&release, &self.update.public_key)?;
        validate_release(&release, &self.id, &self.update.channel)?;
        let existing = self.read_pending()?;
        if version_is_newer(&release.version, &self.version)
            && release_is_soaked(&release.published_at)
            && existing
                .as_ref()
                .is_none_or(|pending| pending.version != release.version)
        {
            let target = update_artifact_target(self.update.install_mode, self.update.package_kind);
            let artifact = release.artifacts.get(&target).ok_or_else(|| {
                ServiceError::Update(format!("release has no artifact for {target}"))
            })?;
            validate_artifact(artifact)?;
            let path = self.root.join(format!(
                "{}.{}",
                release.version,
                artifact_extension(artifact.kind)
            ));
            download_verified(&path, artifact)?;
            if let Some(previous) = &existing {
                sabine_runtime::remove_file_if_present(&previous.artifact)?;
            }
            sabine_runtime::write_json_atomic(
                &self.pending_path(),
                &PendingAppUpdate {
                    app_id: self.id.clone(),
                    version: release.version,
                    artifact: path,
                    sha256: artifact.sha256.clone(),
                    kind: artifact.kind,
                    requires_elevation: artifact.kind.requires_elevation(),
                    staged_at: unix_timestamp(),
                    prompt_after: 0,
                    executable: None,
                    sabine: release.requires_sabine,
                },
            )?;
        } else if let Some(stale) = existing.filter(|pending| !self.is_newer(pending)) {
            sabine_runtime::remove_file_if_present(&stale.artifact)?;
            sabine_runtime::remove_file_if_present(&self.pending_path())?;
        }
        std::fs::write(checked, [])?;
        Ok(())
    }

    /// Checks for updates for as long as the app runs.
    pub fn keep_checking(&self) -> ! {
        loop {
            if let Err(error) = self.check() {
                sabine_runtime::report_error("updates", error);
            }
            std::thread::sleep(default_maintenance_interval());
        }
    }

    /// The downloaded update that is newer than the running app.
    pub fn pending(&self) -> ServiceResult<Option<PendingAppUpdate>> {
        Ok(self
            .read_pending()?
            .filter(|pending| self.is_newer(pending)))
    }

    /// Asks about `pending` again a day from now.
    pub fn defer(&self, mut pending: PendingAppUpdate) -> ServiceResult<()> {
        pending.prompt_after = unix_timestamp().saturating_add(UPDATE_SOAK.as_secs());
        sabine_runtime::write_json_atomic(&self.pending_path(), &pending)?;
        Ok(())
    }

    fn is_newer(&self, pending: &PendingAppUpdate) -> bool {
        version_is_newer(&pending.version, &self.version)
    }

    fn read_pending(&self) -> ServiceResult<Option<PendingAppUpdate>> {
        let path = self.pending_path();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|source| ServiceError::Decode { path, source }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

/// Installs the standalone update staged at `pending` over the app at
/// `install_target`, from a process other than the app's.
pub fn apply_standalone_update(pending: &Path, install_target: &Path) -> ServiceResult<()> {
    let update: PendingAppUpdate =
        serde_json::from_slice(&std::fs::read(pending)?).map_err(|source| {
            ServiceError::Decode {
                path: pending.to_path_buf(),
                source,
            }
        })?;
    verify_sha256(&update.artifact, &update.sha256)?;
    run_package_installer(&update, Some(install_target))?;
    sabine_runtime::remove_file_if_present(pending)?;
    sabine_runtime::remove_file_if_present(&update.artifact)?;
    Ok(())
}

fn modified_within(path: &Path, interval: Duration) -> bool {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .is_ok_and(|modified| {
            SystemTime::now()
                .duration_since(modified)
                .is_ok_and(|age| age < interval)
        })
}
