use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime not found: {0}")]
    NotFound(String),
    #[error("runtime integrity check failed for {path}")]
    IntegrityFailed { path: PathBuf },
    #[error("runtime installation failed: {0}")]
    InstallationFailed(String),
    #[error("{0}")]
    Request(String),
    #[error("{0}")]
    Gone(String),
    #[error("runtime downloads are disabled by configuration")]
    DownloadsDisabled,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
