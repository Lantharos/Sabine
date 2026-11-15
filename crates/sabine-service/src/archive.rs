use std::path::Path;

use crate::{ServiceError, ServiceResult};

pub(crate) fn extract(archive: &Path, destination: &Path) -> ServiceResult<()> {
    sabine_runtime::extract_archive(archive, destination).map_err(|error| {
        ServiceError::Update(format!("could not extract {}: {error}", archive.display()))
    })
}
