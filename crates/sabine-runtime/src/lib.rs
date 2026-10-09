#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
compile_error!("Sabine requires Apple Silicon on macOS");

mod diagnostics;
mod discovery;
mod download;
mod error;
mod file_lock;
mod fingerprint;
mod install;
mod lease;
mod process;
mod types;
#[cfg(windows)]
pub use install::sandbox_windows::prepare_sandbox_access;

pub const MIN_CEF_MAJOR: u32 = 154;

/// Older runtimes hand Linux windows empty shared textures on NVIDIA
/// (chromiumembedded/cef#4237), so they paint in software instead.
#[cfg(target_os = "linux")]
pub const MIN_LINUX_SHARED_TEXTURE_CEF: [u32; 3] = [156, 0, 3];

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use discovery::paths::runtime_execution_path;
pub use discovery::paths::{sabine_data_dir, user_runtime_path};
pub use discovery::resolve::{ensure_runtime, resolve_runtime};
pub use download::latest_install_plan;
pub use download::transfer::download_file as download_file_with_progress;
pub use error::RuntimeError;
pub use file_lock::FileLock;
pub use fingerprint::Fingerprint;
pub use install::archive::extract_tar_archive;
pub use install::assets::prepare_runtime_assets;
pub use install::directory::{install_directory, recover_directory_installs};
pub use install::{
    install_user_runtime_with_progress, prune_user_runtimes, quarantine_user_runtime,
    remove_user_runtime_version, update_user_runtime_with_progress,
};
pub use lease::RuntimeLease;
pub use process::{background_command, configure_background_command, find_program, process_alive};
pub use types::{RuntimeConfig, RuntimeInfo, RuntimeInstallProgress, RuntimeLocation, RuntimeMode};

pub use diagnostics::{capture_diagnostics, diagnostic_path, record_diagnostic, report_error};
pub use discovery::detect::detect_runtime;

pub fn runtime_version_at_least(runtime_dir: &std::path::Path, minimum: &[u32]) -> bool {
    discovery::version::version_sort_key(&discovery::version::detect_version(runtime_dir))
        .as_slice()
        >= minimum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_mode_round_trips() {
        assert_eq!(
            RuntimeMode::SharedPreferred,
            RuntimeMode::parse("shared-preferred").unwrap()
        );
        assert_eq!(
            RuntimeMode::SystemRequired,
            RuntimeMode::parse("system-required").unwrap()
        );
        assert!(RuntimeMode::parse("invalid").is_none());
    }

    #[test]
    fn version_checks_use_major_version() {
        assert!(crate::discovery::version::version_satisfies(
            &format!("{MIN_CEF_MAJOR}.0.14+gabc+chromium-{MIN_CEF_MAJOR}.0.7727.138"),
            MIN_CEF_MAJOR
        ));
        assert!(!crate::discovery::version::version_satisfies(
            "101.0.18+gabc+chromium-101.0.4951.67",
            MIN_CEF_MAJOR
        ));
    }
}
