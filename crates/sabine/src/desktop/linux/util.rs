use std::{env, io, path::PathBuf};

pub(super) fn desktop_value(value: &str) -> String {
    value.replace(['\n', '\r'], " ")
}

/// The directories desktops search for application entries, most specific
/// first.
pub(super) fn application_dirs() -> io::Result<Vec<PathBuf>> {
    let system = env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    Ok(std::iter::once(data_home()?)
        .chain(
            system
                .split(':')
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from),
        )
        .map(|dir| dir.join("applications"))
        .collect())
}

pub(super) fn config_home() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir()?.join(".config"))
}

pub(super) fn data_home() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir()?.join(".local/share"))
}

pub(super) fn home_dir() -> io::Result<PathBuf> {
    env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME is required for Linux desktop integration",
        )
    })
}
