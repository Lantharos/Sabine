pub(crate) mod autostart;
#[cfg(target_os = "linux")]
pub(crate) mod entries;
pub(crate) mod native_messaging;

#[cfg(unix)]
use std::{io, path::PathBuf};

#[cfg(unix)]
pub(crate) fn home_dir() -> io::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

#[cfg(target_os = "linux")]
pub(crate) fn config_home() -> io::Result<PathBuf> {
    match std::env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        Some(path) => Ok(PathBuf::from(path)),
        None => Ok(home_dir()?.join(".config")),
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn data_home() -> io::Result<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        Some(path) => Ok(PathBuf::from(path)),
        None => Ok(home_dir()?.join(".local/share")),
    }
}
