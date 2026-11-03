use crate::{RegisteredApp, SabineService, ServiceError, ServiceResult};

impl SabineService {
    /// Takes `id` off this computer: through the store's own removal when a
    /// store installed it, and otherwise as uninstalling Sabine does.
    pub fn remove_app(&self, id: &str) -> ServiceResult<()> {
        let app = self.app(id)?;
        #[cfg(target_os = "linux")]
        if crate::schelf::installed_by_store(&app) {
            return self.remove_store_app(id);
        }
        remove(self, &app)
    }
}

/// Takes `app` off this computer through the uninstaller its own setup
/// registered, then forgets whatever is left of it. Apps that have no
/// uninstaller of their own, such as ones in development, are only
/// forgotten, and keep their files.
pub(super) fn remove(service: &SabineService, app: &RegisteredApp) -> ServiceResult<()> {
    let id = &app.manifest.id;
    #[cfg(target_os = "windows")]
    super::windows::run_uninstaller(id)?;
    match service.app(id) {
        Ok(_) => {
            let install = app
                .manifest
                .executable
                .parent()
                .unwrap_or(&app.manifest.executable);
            service.forget_app(id, install)
        }
        Err(ServiceError::AppNotFound(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

/// The name people know `app` by, telling a version in development apart
/// from the installed one.
pub(super) fn display_name(app: &RegisteredApp) -> String {
    if crate::AppEnvironment::of(&app.manifest.id) == crate::AppEnvironment::Development {
        format!("{} (in development)", app.manifest.name)
    } else {
        app.manifest.name.clone()
    }
}
