use crate::{RegisteredApp, SabineService, ServiceError, ServiceResult};

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
    if app.manifest.id.ends_with(".dev") {
        format!("{} (in development)", app.manifest.name)
    } else {
        app.manifest.name.clone()
    }
}
