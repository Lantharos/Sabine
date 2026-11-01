use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    TemporaryDirectory, apply_runtime_library_path, apply_runtime_resource_args,
    configure_background_command, prepare_host_execution, runtime_binary_directory,
    validate_host_protocol,
};

pub fn smoke_test_runtime(host: &Path, runtime_dir: &Path) -> Result<(), String> {
    let _lease =
        sabine_runtime::RuntimeLease::acquire(runtime_dir).map_err(|error| error.to_string())?;
    let host = host
        .canonicalize()
        .map_err(|error| format!("could not resolve Sabine host {}: {error}", host.display()))?;
    sabine_runtime::prepare_runtime_assets(runtime_dir).map_err(|error| error.to_string())?;
    validate_host_protocol(&host, runtime_dir)?;
    let host = prepare_host_execution(&host, runtime_dir)?;
    let binary_dir = runtime_binary_directory(runtime_dir);
    let cache_dir = std::env::temp_dir().join(format!(
        "sabine-runtime-probe-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("could not create CEF runtime probe cache: {error}"))?;
    let _cache = TemporaryDirectory(cache_dir.clone());
    let stderr_path = cache_dir.join("probe.stderr");
    let stderr = std::fs::File::create(&stderr_path)
        .map_err(|error| format!("could not create CEF probe diagnostics: {error}"))?;
    let mut command = Command::new(&host);
    configure_background_command(&mut command);
    command
        .arg("--sabine-runtime-smoke-test")
        .arg(format!("--root-cache-path={}", cache_dir.display()))
        .current_dir(&binary_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr);
    apply_runtime_resource_args(&mut command, runtime_dir);
    apply_runtime_library_path(&mut command, runtime_dir)?;
    #[cfg(target_os = "macos")]
    command.arg("--use-mock-keychain");
    // From CEF 156, a headless Chromium whose GPU process can't start renders nothing instead of
    // falling back to software, as on build machines without a display. The probe renders with
    // SwiftShader, which the runtime ships, so it checks the runtime rather than the graphics.
    #[cfg(target_os = "linux")]
    command
        .arg("--headless")
        .arg("--ozone-platform=headless")
        .arg("--use-angle=swiftshader")
        .arg("--password-store=basic");
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start CEF runtime probe: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("could not wait for CEF runtime probe: {error}"))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("CEF runtime probe timed out after 30 seconds".to_string());
        }
        thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = Vec::new();
    if let Ok(file) = std::fs::File::open(stderr_path) {
        let _ = file.take(64 * 1024).read_to_end(&mut stderr);
    }
    let stderr = String::from_utf8_lossy(&stderr);
    if status.success() {
        return Ok(());
    }
    let details = if stderr.trim().is_empty() {
        String::new()
    } else {
        format!(": {}", stderr.trim())
    };
    let sandbox_hint = if cfg!(target_os = "linux")
        && [
            "No usable sandbox",
            "Failed to move to new namespace",
            "SUID sandbox helper",
        ]
        .iter()
        .any(|message| stderr.contains(message))
    {
        "\nChromium's sandbox needs Linux user namespaces. On AppArmor systems, generate a profile with `sabine runtime sandbox-profile` and ask an administrator to install it as described in Sabine's Linux sandbox setup documentation."
    } else {
        ""
    };
    Err(format!(
        "CEF runtime probe exited with {status}{details}{sandbox_hint}"
    ))
}
