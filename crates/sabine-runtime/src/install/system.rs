use std::{fs, io, path::Path};

use super::payload::copy_runtime_payload;

/// Makes `runtime` the only runtime in `runtimes`, a system runtime directory,
/// ready to load without anything writing to it later.
pub fn install_system_runtime(runtime: &Path, runtimes: &Path) -> io::Result<()> {
    let name = runtime
        .canonicalize()?
        .file_name()
        .ok_or_else(|| io::Error::other("runtime has no directory name"))?
        .to_os_string();
    let staging = runtimes.join(".installing").join(&name);
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    copy_runtime_payload(runtime, &staging)?;
    crate::prepare_runtime_assets(&staging)?;
    if !crate::discovery::host::runtime_is_valid(&staging) {
        return Err(io::Error::other(format!(
            "{} is not a complete CEF runtime",
            runtime.display()
        )));
    }
    crate::install_directory(&staging, &runtimes.join(&name))?;
    for entry in fs::read_dir(runtimes)? {
        let entry = entry?;
        if entry.file_name() != name {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}
