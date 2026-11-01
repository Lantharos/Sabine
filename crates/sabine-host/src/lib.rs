// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Windows host probes need CREATE_NO_WINDOW, and a complete host includes its
// DLL and chrome_elf.dll beside the executable. Finding the .exe alone does not
// mean Chromium can start. Resource paths and sandbox access also belong to setup.

//! CEF host embedder sources and cmake build.
//!
//! Apps and the CLI call [`ensure_host`] to materialize `sabine-host` next to
//! a CEF runtime. Process launch and window wiring stay in the `sabine` crate.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

mod build_lock;
mod compile;
mod installed;
#[cfg(target_os = "macos")]
mod macos_execution;
mod probe;
mod protocol;
mod sources;
mod toolchain;
#[cfg(target_os = "windows")]
mod windows_execution;

pub use probe::smoke_test_runtime;
pub use protocol::{HOST_PROTOCOL_VERSION, validate_host_protocol};

const RUNTIME_PROBE_VERSION: u32 = 6;

fn host_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "sabine-host.exe"
    } else {
        "sabine-host"
    }
}

/// The host for a CEF runtime: the one Sabine was handed or ships, a host
/// compiled from the runtime's SDK, or the installed Sabine's when compiling
/// isn't possible or fails.
pub fn ensure_host(runtime_dir: &Path) -> Result<PathBuf, String> {
    sabine_runtime::prepare_runtime_assets(runtime_dir).map_err(|error| error.to_string())?;
    if let Some(host) = available_host(runtime_dir) {
        validate_host_protocol(&host, runtime_dir)?;
        return Ok(host);
    }
    let compile_error = match compile::compile_host(runtime_dir) {
        Ok(host) => return Ok(host),
        Err(error) => error,
    };
    let Some(host) = installed::installed_host() else {
        return Err(format!(
            "Sabine could not compile its host: {compile_error}\nNo Sabine is installed for this user or for every user to provide one, and SABINE_HOST_PATH does not name one."
        ));
    };
    validate_host_protocol(&host, runtime_dir).map_err(|installed_error| {
        format!(
            "Sabine could not compile its host: {compile_error}\nThe installed Sabine host cannot be used either: {installed_error}"
        )
    })?;
    Ok(host)
}

pub fn host_is_complete(path: &Path) -> bool {
    path.is_file()
        && (!cfg!(windows)
            || (path.with_extension("dll").is_file()
                && path.with_file_name("chrome_elf.dll").is_file()))
}

pub fn available_host(runtime_dir: &Path) -> Option<PathBuf> {
    installed::prebuilt_host_path().or_else(|| {
        let path = compile::host_release_binary(runtime_dir);
        host_is_complete(&path).then_some(path)
    })
}

pub fn prepare_host_execution(host: &Path, runtime: &Path) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        macos_execution::prepare(host, runtime)
    }
    #[cfg(target_os = "windows")]
    {
        windows_execution::prepare(host, runtime)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = runtime;
        Ok(host.to_path_buf())
    }
}

pub(crate) fn configure_background_command(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

pub fn runtime_binary_directory(runtime_dir: &Path) -> PathBuf {
    let release = runtime_dir.join("Release");
    if release.is_dir() {
        release
    } else {
        runtime_dir.to_path_buf()
    }
}

struct TemporaryDirectory(PathBuf);

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn apply_runtime_resource_args(command: &mut Command, runtime_dir: &Path) {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let resources = runtime_dir.join("Resources");
        command
            .arg(format!(
                "--sabine-resources-dir-path={}",
                resources.display()
            ))
            .arg(format!(
                "--sabine-locales-dir-path={}",
                resources.join("locales").display()
            ));
    }
    #[cfg(target_os = "windows")]
    command.arg(format!(
        "--sabine-cef-dir={}",
        runtime_binary_directory(runtime_dir).display()
    ));
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let _ = (command, runtime_dir);
}

/// Lets a host started outside an app find the runtime's libcef, which lies
/// beside neither the host nor the app.
fn apply_runtime_library_path(command: &mut Command, runtime_dir: &Path) -> Result<(), String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let variable = if cfg!(target_os = "windows") {
            "PATH"
        } else {
            "LD_LIBRARY_PATH"
        };
        let mut paths = vec![runtime_binary_directory(runtime_dir)];
        if let Some(existing) = std::env::var_os(variable) {
            paths.extend(std::env::split_paths(&existing));
        }
        command.env(
            variable,
            std::env::join_paths(paths).map_err(|error| error.to_string())?,
        );
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let _ = (command, runtime_dir);
    Ok(())
}

pub fn runtime_probe_fingerprint() -> String {
    format!(
        "{}-{RUNTIME_PROBE_VERSION}",
        sources::host_source_fingerprint()
    )
}

#[cfg(test)]
mod tests {
    use super::runtime_binary_directory;

    #[test]
    fn runtime_binary_directory_accepts_flat_platform_layouts() {
        let root =
            std::env::temp_dir().join(format!("sabine-host-runtime-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(runtime_binary_directory(&root), root);
        std::fs::create_dir_all(root.join("Release")).unwrap();
        assert_eq!(runtime_binary_directory(&root), root.join("Release"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
