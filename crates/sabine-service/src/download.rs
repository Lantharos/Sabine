use std::path::Path;

use serde::de::DeserializeOwned;

use crate::{ServiceError, ServiceResult};

const MANIFEST_LIMIT: u64 = 1024 * 1024;
const ARTIFACT_LIMIT: u64 = 8 * 1024 * 1024 * 1024;

pub(crate) fn fetch_manifest<T: DeserializeOwned>(url: &str) -> ServiceResult<T> {
    let body = sabine_runtime::fetch_bytes(url, MANIFEST_LIMIT)
        .map_err(|error| ServiceError::Update(format!("release manifest: {error}")))?;
    serde_json::from_slice(&body)
        .map_err(|error| ServiceError::Update(format!("invalid release manifest: {error}")))
}

/// Downloads a release artifact into place, reporting the bytes received.
pub(crate) fn download_artifact(
    url: &str,
    destination: &Path,
    size: Option<u64>,
    mut on_progress: impl FnMut(u64),
) -> ServiceResult<()> {
    sabine_runtime::download_to(url, destination, size, ARTIFACT_LIMIT, &mut |update| {
        on_progress(update.downloaded)
    })?;
    Ok(())
}

pub(crate) fn verify_sha256(path: &Path, expected: &str) -> ServiceResult<()> {
    let actual = sabine_runtime::sha256_file(path)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(ServiceError::Update(format!(
            "{} does not match its release: expected SHA-256 {expected}, got {actual}",
            path.display()
        )))
    }
}
