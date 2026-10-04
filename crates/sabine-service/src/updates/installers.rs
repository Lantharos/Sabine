use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::app::registry::replace_file;
use crate::types::{AppArtifact, AppArtifactKind, ServiceError, ServiceResult};

pub(super) fn install_archive(
    root: &Path,
    id: &str,
    version: &str,
    artifact: &AppArtifact,
    release_dir: &Path,
) -> ServiceResult<()> {
    let downloads = root.join("downloads").join(id);
    std::fs::create_dir_all(&downloads)?;
    let archive = downloads.join(format!("{version}.archive"));
    download_artifact(&artifact.url, &archive)?;
    verify_sha256(&archive, &artifact.sha256)?;
    let parent = release_dir
        .parent()
        .ok_or_else(|| ServiceError::Update("release directory has no parent".to_string()))?;
    let staging = parent.join(".staging").join(version);
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    let executable = artifact.executable.as_ref().ok_or_else(|| {
        ServiceError::Update("managed update artifact has no executable".to_string())
    })?;
    if !safe_relative_path(executable) {
        return Err(ServiceError::Update(
            "managed executable must be a relative path".to_string(),
        ));
    }
    let installed = (|| -> ServiceResult<()> {
        crate::archive::extract(&archive, &staging)?;
        validate_executable(&staging.join(executable))?;
        sabine_runtime::install_directory(&staging, release_dir)?;
        Ok(())
    })();
    if installed.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    installed?;
    let _ = std::fs::remove_file(archive);
    Ok(())
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
        File::open(path)?.read_exact(&mut signature)?;
        if signature != *b"MZ" {
            return Err(ServiceError::Update(format!(
                "app executable is not a Windows program: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

pub(super) fn download_artifact(url: &str, destination: &Path) -> ServiceResult<()> {
    let temporary = destination.with_extension("download");
    sabine_runtime::download_file_with_progress(
        url,
        &temporary,
        None,
        8 * 1024 * 1024 * 1024,
        &mut |_| {},
    )?;
    replace_file(&temporary, destination)?;
    Ok(())
}

pub(super) fn verify_sha256(path: &Path, expected: &str) -> ServiceResult<()> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(ServiceError::Update(
            "artifact SHA-256 mismatch".to_string(),
        ))
    }
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

pub(super) fn write_json_atomic(path: &Path, value: &impl serde::Serialize) -> ServiceResult<()> {
    let temporary = path.with_extension("new");
    let mut file = File::create(&temporary)?;
    file.write_all(&serde_json::to_vec_pretty(value).expect("pending update is serializable"))?;
    file.sync_all()?;
    replace_file(&temporary, path)?;
    Ok(())
}

pub(crate) fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}
