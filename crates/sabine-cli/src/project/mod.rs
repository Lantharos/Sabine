mod app;
mod cargo;
pub(crate) mod environment;
mod file;
mod updates;
mod web;

use std::path::{Path, PathBuf};

pub(crate) use app::{BundleApp, ConfigOverrides, detect_icon, resolve_app};
pub(crate) use cargo::CargoPackage;
pub(crate) use file::read_sabine_file;

/// An app id made from a name: lowercase letters, digits, dots and hyphens.
pub(crate) fn sanitize_id(value: &str) -> String {
    let output = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let output = output.trim_matches('-').to_string();
    if output.is_empty() {
        "app".to_string()
    } else {
        output
    }
}

/// `path` from the working directory, which must exist.
pub(crate) fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    dunce::canonicalize(&path)
        .map_err(|error| format!("source path {} is unavailable: {error}", path.display()))
}
