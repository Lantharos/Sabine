use std::path::Path;

use sabine_runtime::RuntimeInfo;

use super::{BuildTarget, BundleFormat, StagedBundle};
use crate::bundle::{linux, windows::WindowsArchitecture};

/// Bundles that carry Chromium carry this machine's, so they must be built
/// on the operating system and CPU architecture they are for.
pub(super) fn validate_platform(format: BundleFormat, binary: &Path) -> Result<(), String> {
    let target = format.target();
    if target.as_str() != std::env::consts::OS {
        return Err(
            "bundles that carry Chromium must be built on their target operating system".into(),
        );
    }
    let architecture = match target {
        BuildTarget::Macos => "aarch64",
        BuildTarget::Windows => WindowsArchitecture::of(binary)?.rust_name(),
        BuildTarget::Linux => linux::package::architecture(binary)?.1,
    };
    if architecture != std::env::consts::ARCH {
        return Err(format!(
            "{architecture} bundles that carry Chromium must be built on a {architecture} machine"
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
    let runtime = prepared_runtime()?;
    let host = sabine_host::ensure_host(runtime.location.path())?;
    let service = sabine_service::ensure_service_executable(|_| {})
        .map_err(|error| format!("could not prepare the Sabine service to bundle: {error}"))?;
    sabine_service::stage_system_files(binary_dir(staged)?, &service, &host, None)
        .map_err(|error| format!("could not stage the Sabine service: {error}"))?;
    stage_runtime(format, staged, &runtime)
}

/// Ships this machine's CEF host and runtime inside a standalone app, which
/// runs on them alone.
pub(super) fn stage_standalone_runtime(
    format: BundleFormat,
    staged: &StagedBundle,
) -> Result<(), String> {
    let runtime = prepared_runtime()?;
    let host = sabine_host::ensure_host(runtime.location.path())?;
    sabine_service::stage_host(binary_dir(staged)?, &host)
        .map_err(|error| format!("could not stage the Sabine host: {error}"))?;
    stage_runtime(format, staged, &runtime)
}

fn prepared_runtime() -> Result<RuntimeInfo, String> {
    crate::runtime::ensure_runtime()
        .map_err(|error| format!("could not prepare the CEF runtime to bundle: {error}"))
}

fn binary_dir(staged: &StagedBundle) -> Result<&Path, String> {
    staged
        .binary
        .parent()
        .ok_or_else(|| "the application has no binary directory".to_string())
}

fn stage_runtime(
    format: BundleFormat,
    staged: &StagedBundle,
    runtime: &RuntimeInfo,
) -> Result<(), String> {
    let manifest_dir = match format.target() {
        BuildTarget::Macos => staged.app_dir.join("Contents/Resources"),
        _ => binary_dir(staged)?.join("resources"),
    };
    let runtime_name = runtime
        .location
        .path()
        .file_name()
        .ok_or_else(|| "the CEF runtime has no directory name".to_string())?;
    let destination = manifest_dir.join("runtimes/cef").join(runtime_name);
    sabine_runtime::copy_runtime_payload(runtime.location.path(), &destination)
        .map_err(|error| format!("could not stage the CEF runtime: {error}"))
}
