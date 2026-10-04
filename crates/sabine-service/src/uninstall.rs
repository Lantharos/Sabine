// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Stop the verified daemon before removing its binaries. A running Windows
// uninstaller must move itself outside that directory before deleting it.

use crate::{SabineService, ServiceError, ServiceResult, app::forget::remove_path};
use sabine_runtime::sabine_data_dir;
use std::time::Duration;

pub fn uninstall_system(purge: bool) -> ServiceResult<bool> {
    let root = sabine_data_dir();
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
    let lock = sabine_runtime::FileLock::acquire(
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
    let removed = if purge {
        root.clone()
    } else {
        root.join("bin")
    };
    let removed_self = std::env::current_exe()?.starts_with(&removed);
    if removed_self {
        self_replace::self_delete_outside_path(&removed)?;
    }
    if purge {
        drop(lock);
        remove_path(&crate::app::data::browser_profiles_root())?;
        remove_path(&root)?;
    } else {
        for path in [
            removed,
            root.join("downloads/system"),
            root.join("downloads/cli"),
            root.join("executions"),
        ] {
            remove_path(&path)?;
        }
    }
    Ok(removed_self)
}
