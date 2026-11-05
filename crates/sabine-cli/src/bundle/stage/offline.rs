use std::path::Path;

use super::{BuildTarget, BundleFormat, StagedBundle};
use crate::bundle::{linux, windows::WindowsArchitecture};

pub(super) fn validate_platform(format: BundleFormat, binary: &Path) -> Result<(), String> {
    let target = format.target();
    if target.as_str() != std::env::consts::OS {
        return Err("offline bundles must be built on their target operating system".into());
    }
    let architecture = match target {
        BuildTarget::Macos => "aarch64",
        BuildTarget::Windows => WindowsArchitecture::of(binary)?.rust_name(),
        BuildTarget::Linux => linux::package::architecture(binary)?.1,
    };
    if architecture != std::env::consts::ARCH {
        return Err(format!(
            "offline {architecture} bundles must be built on a {architecture} machine"
        ));
    }
    Ok(())
}

/// Ships this machine's Sabine service, host and CEF runtime inside the
/// bundle, so the app starts without downloading them.
pub(super) fn stage_offline_runtime(
    format: BundleFormat,
    staged: &StagedBundle,
) -> Result<(), String> {
    let runtime = crate::runtime::ensure_runtime()
        .map_err(|error| format!("could not prepare offline CEF runtime: {error}"))?;
    let host = sabine_host::ensure_host(runtime.location.path())?;
    let service = sabine_service::ensure_service_executable(|_| {})
        .map_err(|error| format!("could not prepare offline Sabine service: {error}"))?;
    let binary_dir = staged
        .binary
        .parent()
        .ok_or("offline application has no binary directory")?;
    sabine_service::stage_system_files(binary_dir, &service, &host, None)
        .map_err(|error| format!("could not stage the offline Sabine service: {error}"))?;
    let manifest_dir = match format.target() {
        BuildTarget::Macos => staged.app_dir.join("Contents/Resources"),
        _ => binary_dir.join("resources"),
    };
    let runtime_name = runtime
        .location
        .path()
        .file_name()
        .ok_or_else(|| "offline runtime has no directory name".to_string())?;
    let destination = manifest_dir.join("runtimes/cef").join(runtime_name);
    sabine_runtime::copy_runtime_payload(runtime.location.path(), &destination)
        .map_err(|error| format!("could not stage offline CEF runtime: {error}"))
}
