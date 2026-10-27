//! Running inside an app sandbox that marks its processes with `/.flatpak-info`,
//! such as Flatpak's or Luft's. Chromium can't create the namespaces for its
//! renderer sandbox there, so it starts its helpers through Zypak, which asks
//! the sandbox's Spawn portal for them.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const SANDBOX_INFO: &str = "/.flatpak-info";
const ZYPAK_BIN: &str = "ZYPAK_BIN";
const ZYPAK_HELPER: &str = "zypak-helper";
const LIBCEF: &str = "libcef.so";

pub(crate) fn in_app_sandbox() -> bool {
    Path::new(SANDBOX_INFO).exists()
}

fn zypak_helper() -> Option<PathBuf> {
    in_app_sandbox()
        .then(|| std::env::var_os(ZYPAK_BIN))
        .flatten()
        .filter(|folder| !folder.is_empty())
        .map(|folder| PathBuf::from(folder).join(ZYPAK_HELPER))
}

/// The command that starts the app's own Chromium: the host itself, or the
/// host through Zypak inside an app sandbox that provides it. Zypak starts the
/// helpers from `/` without the host's library path, so libcef reaches them
/// preloaded.
pub(crate) fn browser_command(host: &Path, binary_dir: &Path) -> Command {
    let Some(helper) = zypak_helper() else {
        return Command::new(host);
    };
    let libcef: OsString = binary_dir.join(LIBCEF).into();
    let mut command = Command::new(helper);
    command
        .arg("host")
        .arg("-")
        .arg(host)
        .env("ZYPAK_LD_PRELOAD", &libcef)
        .env("ZYPAK_CEF_LIBRARY_PATH", &libcef);
    command
}
