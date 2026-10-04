mod app;
mod archive;
mod http;
mod install;
mod lifecycle;
mod release;
mod types;
mod uninstall;
mod updates;
#[cfg(windows)]
pub mod windows_registry;

pub use app::data::browser_profile_path;
#[cfg(windows)]
pub use app::desktop::autostart::APP_AUTOSTART_KEY;
#[cfg(target_os = "macos")]
pub use app::desktop::autostart::app_autostart_label;
#[cfg(unix)]
pub use app::desktop::autostart::app_autostart_path;
#[cfg(windows)]
pub use app::desktop::native_messaging::native_messaging_registry_keys;
pub use app::desktop::native_messaging::{NativeMessagingBrowser, native_messaging_manifest_dirs};
pub use app::environment::AppEnvironment;
pub use app::payload::remove_app_payload;
pub use uninstall::uninstall_system;

pub use app::registry::SabineService;
pub(crate) use install::{
    StagedSystemUpdate, cached_service_path, find_service_executable, repair_system_installation,
    rollback_system_update, stage_system_update,
};
pub use install::{
    ensure_service_executable, installed_service_version, service_daemon_path, update_components,
};
pub(crate) use lifecycle::{PrepareProgress, PrepareStage};
pub use lifecycle::{
    adopt_with_runtime, complete_system_update, ensure_daemon_running, ensure_ready, load_policy,
    prepare_machine_with_progress, resolve_service_executable, run_daemon, running_daemon_version,
    set_login_autostart,
};
pub(crate) use release::rollout::release_is_soaked;
pub use release::signing::{public_key_from_private, sign_app_release, sign_system_release};
pub(crate) use release::signing::{verify_app_release, verify_system_release};
pub use types::{
    APPIMAGE_PROGRAM_ENV, AppArtifact, AppArtifactKind, AppInstallMode, AppManifest,
    AppReleaseManifest, AppUpdateConfig, AppUpdateSource, AppUpdateStatus, SABINE_VERSION,
    SabineVersion, ServiceError, SystemCompatibility, SystemReleaseArtifact, SystemReleaseManifest,
    UpdatePolicy, valid_app_id,
};
pub(crate) use types::{
    RegisteredApp, ServiceResult, UPDATE_ROLLOUT_WINDOW, UPDATE_SOAK, default_maintenance_interval,
};
pub use updates::retry_quarantined_runtimes;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn service() -> SabineService {
        SabineService::new(std::env::temp_dir().join(format!(
            "sabine-service-test-{}-{}",
            std::process::id(),
            types::unix_timestamp()
        )))
    }

    fn manifest() -> AppManifest {
        AppManifest {
            id: "net.lantharos.notes".to_string(),
            name: "Notes".to_string(),
            version: "1.0.0".to_string(),
            executable: std::path::PathBuf::from("/opt/notes/notes"),
            args: Vec::new(),
            update: Some(AppUpdateConfig {
                source: AppUpdateSource::Http {
                    url: "https://updates.example.test/notes.json".to_string(),
                },
                channel: "stable".to_string(),
                policy: UpdatePolicy::Automatic,
                install_mode: AppInstallMode::Managed,
                public_key: "VXtTlN3HZuGwYByjJu+3HQGavjJwRo0i9/RGrT6Ua6M=".to_string(),
                package_kind: None,
            }),
            sabine: SabineVersion::current(),
        }
    }

    #[test]
    fn registry_round_trips_apps() {
        let service = service();
        let registered = service.register(manifest()).unwrap();
        assert_eq!(registered.manifest.id, "net.lantharos.notes");
        assert_eq!(service.apps().unwrap().len(), 1);
        assert_eq!(service.app("net.lantharos.notes").unwrap(), registered);
        assert_eq!(
            service.unregister("net.lantharos.notes").unwrap(),
            registered
        );
    }

    #[test]
    fn registry_rejects_insecure_update_urls() {
        let service = service();
        let mut app = manifest();
        app.update.as_mut().unwrap().source = AppUpdateSource::Http {
            url: "http://example.test/app.json".to_string(),
        };
        assert!(matches!(
            service.register(app),
            Err(ServiceError::InvalidManifest(_))
        ));
    }

    #[test]
    fn update_paths_stay_inside_release_directory() {
        assert!(updates::safe_relative_path(Path::new("bin/notes")));
        assert!(!updates::safe_relative_path(Path::new("../notes")));
        assert!(!updates::safe_relative_path(Path::new("/usr/bin/notes")));
    }

    #[test]
    fn update_versions_follow_semver_precedence() {
        assert!(types::version_is_newer("1.10.0", "1.9.9"));
        assert!(types::version_is_newer("1.2.0", "1.2.0-beta.1"));
        assert!(!types::version_is_newer("1.2.0-beta.1", "1.2.0"));
        assert!(!types::version_is_newer("1.2.0", "1.2.0"));
        assert!(!types::version_is_newer("1.1.9", "1.2.0"));
        assert!(!types::version_is_newer("latest", "1.2.0"));
    }

    #[test]
    fn runtime_quarantine_is_scoped_to_the_host_build() {
        assert!(updates::quarantine_belongs_to_host(
            "probe=current\nprobe failed",
            "probe=current"
        ));
        assert!(!updates::quarantine_belongs_to_host(
            "probe=previous\nprobe failed",
            "probe=current"
        ));
        assert!(!updates::quarantine_belongs_to_host(
            "legacy probe failure",
            "host=current"
        ));
    }
}
