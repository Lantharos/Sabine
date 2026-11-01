use sabine_runtime::{
    prune_user_runtimes, quarantine_user_runtime, resolve_runtime,
    update_user_runtime_with_progress,
};

use crate::app::registry::SabineService;
use crate::types::{AppUpdateStatus, MaintenanceReport, ServiceError, ServiceResult, UpdatePolicy};

mod apps;
pub(crate) mod installers;
mod packages;

use apps::{ArchiveUpdates, SystemUpdates};

impl SabineService {
    pub fn maintain(&self) -> ServiceResult<MaintenanceReport> {
        let mut update_failures = Vec::new();
        let runtime = match self.update_runtime() {
            Ok(runtime) => Some(runtime),
            Err(error) => {
                update_failures.push(format!("runtime: {error}"));
                resolve_runtime(&self.runtime).ok()
            }
        };
        let pruned_runtimes = match prune_user_runtimes(2) {
            Ok(count) => count,
            Err(error) => {
                update_failures.push(format!("runtime pruning: {error}"));
                0
            }
        };
        let incompatible_apps = self.incompatible_apps()?;
        let apps = self.apps()?;
        let mut updated_apps = Vec::new();
        let mut pending_apps = Vec::new();
        let mut required_system_update: Option<crate::SystemRequirement> = None;
        for app in &apps {
            let Some(update) = &app.manifest.update else {
                continue;
            };
            let archive_updates = match update.policy {
                UpdatePolicy::Automatic => ArchiveUpdates::Install,
                UpdatePolicy::Notify => ArchiveUpdates::AskFirst,
            };
            let require_soak = !incompatible_apps.contains(&app.manifest.id);
            let result = self.update_app_as(
                &app.manifest.id,
                require_soak,
                SystemUpdates::Report,
                archive_updates,
            );
            match result {
                Ok(AppUpdateStatus::Installed { .. }) => updated_apps.push(app.manifest.id.clone()),
                Ok(AppUpdateStatus::PendingApproval(_)) => {
                    pending_apps.push(app.manifest.id.clone())
                }
                Ok(AppUpdateStatus::RequiresSystem { sabine, .. }) => {
                    let required =
                        required_system_update.get_or_insert_with(|| crate::SystemRequirement {
                            sabine,
                            apps: Vec::new(),
                        });
                    required.sabine = required.sabine.max(sabine);
                    required.apps.push(app.manifest.id.clone());
                }
                Ok(
                    AppUpdateStatus::Current
                    | AppUpdateStatus::Deferred { .. }
                    | AppUpdateStatus::StoreManaged,
                ) => {}
                Err(error) => update_failures.push(format!("{}: {error}", app.manifest.id)),
            }
        }
        Ok(MaintenanceReport {
            runtime,
            pruned_runtimes,
            registered_apps: apps.len(),
            updating_apps: apps
                .iter()
                .filter(|app| app.manifest.update.is_some())
                .count(),
            updated_apps,
            pending_apps,
            update_failures,
            incompatible_apps,
            required_system_update,
        })
    }

    pub fn update_runtime(&self) -> ServiceResult<sabine_runtime::RuntimeInfo> {
        self.update_runtime_with_progress(|_| {})
    }

    pub fn update_runtime_with_progress(
        &self,
        progress: impl FnMut(sabine_runtime::RuntimeInstallProgress),
    ) -> ServiceResult<sabine_runtime::RuntimeInfo> {
        retry_quarantined_runtimes()?;
        let shared = matches!(
            self.runtime.mode,
            sabine_runtime::RuntimeMode::SharedPreferred
                | sabine_runtime::RuntimeMode::SystemPreferred
        );
        let runtime = if shared && let Some(runtime) = self.system_runtime() {
            runtime
        } else if shared && self.runtime.allow_user_install {
            update_user_runtime_with_progress(&self.runtime, progress)?
        } else {
            resolve_runtime(&self.runtime)?
        };
        let host = sabine_host::ensure_host(runtime.location.path()).map_err(|error| {
            ServiceError::Update(format!(
                "the Sabine host to validate the runtime with is unavailable: {error}"
            ))
        })?;
        if let Err(error) = sabine_host::smoke_test_runtime(&host, runtime.location.path()) {
            quarantine_user_runtime(
                &runtime,
                &format!(
                    "probe={}\n{error}",
                    sabine_host::runtime_probe_fingerprint()
                ),
            )?;
            return Err(ServiceError::Update(error));
        }
        Ok(runtime)
    }

    /// The runtime the machine provides for every user, which its
    /// administrator keeps up to date in place of per-user downloads.
    fn system_runtime(&self) -> Option<sabine_runtime::RuntimeInfo> {
        resolve_runtime(&sabine_runtime::RuntimeConfig {
            mode: sabine_runtime::RuntimeMode::SystemRequired,
            ..self.runtime.clone()
        })
        .ok()
    }
}
pub fn retry_quarantined_runtimes() -> ServiceResult<()> {
    let root = sabine_runtime::user_runtime_path();
    let current = format!("probe={}", sabine_host::runtime_probe_fingerprint());
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let marker = entry.path().join(".sabine-unusable");
        let Ok(reason) = std::fs::read_to_string(&marker) else {
            continue;
        };
        if !quarantine_belongs_to_host(&reason, &current) {
            std::fs::remove_file(marker)?;
        }
    }
    Ok(())
}

pub(super) fn quarantine_belongs_to_host(reason: &str, current: &str) -> bool {
    reason.lines().next() == Some(current)
}
