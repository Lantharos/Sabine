use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What `sabine install --bundle --system-wide` placed on the machine, so
/// `sabine uninstall` can take it off again.
#[derive(Deserialize, Serialize)]
pub(crate) struct SystemInstall {
    pub name: String,
    pub paths: Vec<PathBuf>,
}

impl SystemInstall {
    pub(crate) fn write(&self, id: &str, root: Option<&Path>) -> Result<(), String> {
        let path = sabine_runtime::path_in_root(root, &record_path(id));
        std::fs::create_dir_all(path.parent().expect("the record has a folder"))
            .map_err(|error| error.to_string())?;
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("could not write {}: {error}", path.display()))
    }

    pub(crate) fn read(id: &str) -> Result<Option<Self>, String> {
        let path = record_path(id);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| format!("could not read {}: {error}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Removes the app for every user, which needs administrator rights.
    pub(crate) fn remove(&self, id: &str) -> Result<(), String> {
        for path in &self.paths {
            let removed = match std::fs::symlink_metadata(path) {
                Ok(metadata) if metadata.is_dir() => {
                    sabine_runtime::remove_directory_if_present(path)
                }
                Ok(_) => std::fs::remove_file(path),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            };
            removed.map_err(|error| format!("could not remove {}: {error}", path.display()))?;
        }
        #[cfg(target_os = "linux")]
        {
            sabine_service::refresh_desktop_database(Path::new("/usr/share/applications"));
            sabine_service::refresh_mime_database(Path::new("/usr/share/mime"));
            sabine_service::refresh_icon_cache(Path::new("/usr/share/icons/hicolor"));
        }
        let record = record_path(id);
        std::fs::remove_file(&record)
            .map_err(|error| format!("could not remove {}: {error}", record.display()))
    }
}

fn record_path(id: &str) -> PathBuf {
    sabine_runtime::system_dir()
        .join("installed-apps")
        .join(format!("{id}.json"))
}
