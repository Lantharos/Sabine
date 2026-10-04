use std::{fs, io, path::Path};

use super::desktop::{
    autostart::remove_app_autostart, native_messaging::remove_native_messaging_hosts,
};
use crate::{SabineService, ServiceError, ServiceResult};

impl SabineService {
    /// Removes everything Sabine keeps for an app outside its installed files:
    /// the registration, login item, browser hosts launching from `install`,
    /// managed releases, staged updates and downloads. Browser profiles stay.
    pub fn forget_app(&self, id: &str, install: &Path) -> ServiceResult<()> {
        let directory = self.root.join("apps").join(id);
        let releases = directory.join("releases");
        let lock = self.app_update_lock(id)?;
        remove_native_messaging_hosts(&[install, &releases])?;
        remove_app_autostart(id)?;
        for path in [
            releases,
            directory.join("pending-update.json"),
            self.root.join("downloads").join(id),
        ] {
            remove_path(&path)?;
        }
        match self.unregister(id) {
            Ok(_) | Err(ServiceError::AppNotFound(_)) => {}
            Err(error) => return Err(error),
        }
        drop(lock);
        remove_path(&directory.join("update.lock"))?;
        match fs::remove_dir(&directory) {
            Err(error)
                if !matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                Err(error.into())
            }
            _ => Ok(()),
        }
    }
}

pub(crate) fn remove_path(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
