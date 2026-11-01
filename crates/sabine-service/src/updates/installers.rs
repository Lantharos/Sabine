use std::path::{Path, PathBuf};

use crate::types::{AppArtifactKind, ServiceError, ServiceResult};

/// Unpacks a verified release archive into `release_dir`, checking that it
/// holds `executable`.
pub(super) fn install_archive(
    archive: &Path,
    executable: &Path,
    release_dir: &Path,
) -> ServiceResult<()> {
    if !safe_relative_path(executable) {
        return Err(ServiceError::Update(
            "managed executable must be a relative path".to_string(),
        ));
    }
    let (Some(parent), Some(version)) = (release_dir.parent(), release_dir.file_name()) else {
        return Err(ServiceError::Update(
            "release directory has no parent".to_string(),
        ));
    };
    let staging = parent.join(".staging").join(version);
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    let installed = (|| -> ServiceResult<()> {
        crate::archive::extract(archive, &staging)?;
        validate_executable(&staging.join(executable))?;
        sabine_runtime::install_directory(&staging, release_dir)?;
        Ok(())
    })();
    if installed.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    installed
}

pub(crate) fn validate_executable(path: &Path) -> ServiceResult<()> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        ServiceError::Update(format!(
            "invalid app executable {}: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(ServiceError::Update(format!(
            "app executable is not a nonempty file: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(ServiceError::Update(format!(
                "app executable has no execute permission: {}",
                path.display()
            )));
        }
    }
    #[cfg(windows)]
    {
        let mut signature = [0_u8; 2];
        use std::io::Read;
        std::fs::File::open(path)?.read_exact(&mut signature)?;
        if signature != *b"MZ" {
            return Err(ServiceError::Update(format!(
                "app executable is not a Windows program: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

pub(super) fn artifact_extension(kind: AppArtifactKind) -> &'static str {
    match kind {
        AppArtifactKind::Archive => "archive",
        AppArtifactKind::Deb => "deb",
        AppArtifactKind::Rpm => "rpm",
        AppArtifactKind::Msi => "msi",
        AppArtifactKind::Exe => "exe",
        AppArtifactKind::Dmg => "dmg",
        AppArtifactKind::AppImage => "AppImage",
    }
}

pub(super) fn pending_path(root: &Path, id: &str) -> PathBuf {
    root.join("apps").join(id).join("pending-update.json")
}

pub(crate) fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}
