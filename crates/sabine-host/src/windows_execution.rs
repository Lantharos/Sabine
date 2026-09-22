// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Chromium's bootstrap and chrome_elf must come from the selected CEF runtime.
// Matching sandbox/API hashes do not make an older bootstrap safe: mixing
// 152.0.7's bootstrap with 152.0.8's libcef crashes during initialization.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn prepare(host: &Path, runtime: &Path) -> Result<PathBuf, String> {
    let binaries = crate::runtime_binary_directory(runtime);
    let files = [
        (binaries.join("bootstrap.exe"), "sabine-host.exe"),
        (binaries.join("chrome_elf.dll"), "chrome_elf.dll"),
        (host.with_extension("dll"), "sabine-host.dll"),
    ];
    let mut fingerprint = DefaultHasher::new();
    for source in files.iter().map(|(source, _)| source) {
        let source = source.canonicalize().map_err(|error| {
            format!(
                "could not resolve Windows host component {}: {error}",
                source.display()
            )
        })?;
        let metadata = source.metadata().map_err(|error| error.to_string())?;
        source.hash(&mut fingerprint);
        metadata.len().hash(&mut fingerprint);
        metadata
            .modified()
            .map_err(|error| error.to_string())?
            .hash(&mut fingerprint);
    }
    let cache =
        sabine_runtime::runtime_execution_path(runtime).map_err(|error| error.to_string())?;
    let directory = cache.join(format!("windows-1-{:016x}", fingerprint.finish()));
    let executable = directory.join("sabine-host.exe");
    let ready = || directory.join("ready").is_file() && crate::host_is_complete(&executable);
    if ready() {
        return Ok(executable);
    }
    let _lock = sabine_runtime::FileLock::acquire(
        &cache.join(".assembly.lock"),
        Duration::from_secs(600),
        |_| {},
    )
    .map_err(|error| error.to_string())?;
    if ready() {
        return Ok(executable);
    }
    let staging = directory.with_extension("installing");
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    let result = (|| {
        fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
        sabine_runtime::prepare_sandbox_access(&staging, true)?;
        for (source, name) in &files {
            let target = staging.join(name);
            fs::copy(source, &target).map_err(|error| {
                format!(
                    "could not stage Windows host component {}: {error}",
                    source.display()
                )
            })?;
            sabine_runtime::prepare_sandbox_access(&target, false)?;
        }
        fs::write(staging.join("ready"), []).map_err(|error| error.to_string())?;
        sabine_runtime::install_directory(&staging, &directory)
            .map_err(|error| error.to_string())?;
        Ok(executable)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}
