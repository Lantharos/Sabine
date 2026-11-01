use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::{
    host_is_complete, sources::write_host_source, toolchain::apply_cmake_generator,
    validate_host_protocol,
};

const BUILD_LOCK_TIMEOUT: Duration = Duration::from_secs(600);

pub(crate) fn host_release_binary(runtime_dir: &Path) -> PathBuf {
    let root = runtime_dir
        .join(".sabine-hosts")
        .join(crate::sources::host_source_fingerprint());
    if cfg!(target_os = "macos") {
        root.join("sabine-host.app/Contents/MacOS/sabine-host")
    } else {
        root.join(crate::host_binary_name())
    }
}

/// Compiles the host from the runtime's CEF SDK, or takes the one another
/// process compiled from these sources meanwhile.
pub(crate) fn compile_host(runtime_dir: &Path) -> Result<PathBuf, String> {
    let missing = missing_sdk_parts(runtime_dir);
    if !missing.is_empty() {
        return Err(format!(
            "the CEF runtime at {} has no build SDK (missing {}); compiling the host needs a Minimal or Standard CEF SDK",
            runtime_dir.display(),
            missing.join(", ")
        ));
    }
    if sabine_runtime::find_program("cmake").is_none() {
        return Err("cmake is not installed or not on PATH".into());
    }
    let binary = host_release_binary(runtime_dir);
    let _lock = sabine_runtime::FileLock::acquire(
        &runtime_dir.join(".sabine-host-build.lock"),
        BUILD_LOCK_TIMEOUT,
        |_| {},
    )
    .map_err(|error| format!("could not lock the host build: {error}"))?;
    if !host_is_complete(&binary) {
        run_build(runtime_dir)?;
    }
    if !host_is_complete(&binary) {
        return Err(format!(
            "the host build did not create {}",
            binary.display()
        ));
    }
    validate_host_protocol(&binary, runtime_dir)?;
    Ok(binary)
}

fn missing_sdk_parts(runtime_dir: &Path) -> Vec<&'static str> {
    [
        ("cmake", runtime_dir.join("cmake").is_dir()),
        ("include", runtime_dir.join("include").is_dir()),
        (
            "include/cef_version.h",
            runtime_dir.join("include").join("cef_version.h").is_file(),
        ),
        ("libcef_dll", runtime_dir.join("libcef_dll").is_dir()),
    ]
    .into_iter()
    .filter_map(|(name, present)| (!present).then_some(name))
    .collect()
}

fn run_build(runtime_dir: &Path) -> Result<(), String> {
    let fingerprint = crate::sources::host_source_fingerprint();
    let work_dir = runtime_dir.join(".sabine-host-build").join(&fingerprint);
    let source_dir = work_dir.join("src");
    let build_dir = work_dir.join("build");
    if work_dir.exists() {
        std::fs::remove_dir_all(&work_dir).map_err(|error| error.to_string())?;
    }
    std::fs::create_dir_all(&source_dir).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&build_dir).map_err(|error| error.to_string())?;
    write_host_source(&source_dir)?;

    let mut configure = Command::new("cmake");
    configure
        .arg("-S")
        .arg(&source_dir)
        .arg("-B")
        .arg(&build_dir);
    apply_cmake_generator(&mut configure)?;
    let cef_root = to_cmake_path(runtime_dir);
    let output_dir = to_cmake_path(&runtime_dir.join(".sabine-hosts").join(fingerprint));
    configure
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg(format!("-DCEF_ROOT={cef_root}"))
        .arg(format!("-DSABINE_HOST_OUTPUT_DIR={output_dir}"));
    run_checked(&mut configure)?;
    run_checked(
        Command::new("cmake")
            .arg("--build")
            .arg(&build_dir)
            .arg("--config")
            .arg("Release")
            .arg("--target")
            .arg("sabine-host")
            .arg("--parallel"),
    )
}

/// CMake strings treat backslashes as escapes, so `\U` in a Windows path would
/// break CEF's build macros.
fn to_cmake_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn run_checked(command: &mut Command) -> Result<(), String> {
    crate::configure_background_command(command);
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "command failed: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}
