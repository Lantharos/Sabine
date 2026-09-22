// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Stop the verified daemon before removing its binaries. A running Windows
// uninstaller must move itself outside that directory before deleting it.

use crate::{SabineService, ServiceError, ServiceResult, service_data_dir};
use std::{fs, path::Path, time::Duration};

pub fn uninstall_system(purge: bool) -> ServiceResult<bool> {
    let root = service_data_dir();
    let apps = SabineService::default().apps()?;
    if !apps.is_empty() {
        return Err(ServiceError::Update(format!(
            "uninstall these apps before removing their shared Sabine runtime: {}",
            apps.iter()
                .map(|app| app.manifest.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let _lock = sabine_runtime::FileLock::acquire(
        &root.join("manual-update.lock"),
        Duration::from_secs(600),
        |_| {},
    )?;
    crate::set_login_autostart(false)?;
    crate::lifecycle::stop_daemon()?;
    for runtime in sabine_runtime::detect_runtime(&Default::default()) {
        if matches!(
            runtime.location,
            sabine_runtime::RuntimeLocation::UserLocal(_)
        ) && !sabine_runtime::remove_user_runtime_version(&runtime.version)?
        {
            return Err(ServiceError::Update(format!(
                "CEF {} is in use; close Sabine apps and retry",
                runtime.version
            )));
        }
    }
    let executable = std::env::current_exe()?;
    let bin = root.join("bin");
    let removed_self = executable.starts_with(&bin);
    if removed_self {
        self_replace::self_delete_outside_path(&bin)?;
    }
    for path in [
        bin,
        root.join("downloads/system"),
        root.join("downloads/cli"),
        root.join("executions"),
    ] {
        remove_path(&path)?;
    }
    if purge {
        remove_path(&crate::app_data::browser_profiles_root())?;
        for name in [
            "logs",
            "apps",
            "downloads",
            "apps.json",
            "apps.json.bak",
            "service-policy.json",
            "update-rollout-offset",
        ] {
            remove_path(&root.join(name))?;
        }
    }
    Ok(removed_self)
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
