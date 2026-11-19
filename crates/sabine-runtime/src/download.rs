pub(crate) mod http;
pub(crate) mod index;
pub(crate) mod transfer;

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use sha1::Sha1;

use crate::error::RuntimeError;
use crate::types::{RuntimeInstallProgress, RuntimeInstallStep};

pub(crate) fn download_file(
    url: &str,
    destination: &Path,
    size: u64,
    progress: &mut impl FnMut(RuntimeInstallProgress),
) -> Result<(), RuntimeError> {
    transfer::download_file(url, destination, Some(size), size, &mut |update| {
        let portion = update.downloaded as f32 / size as f32;
        let message = if update.attempt > 1 {
            format!("Resuming runtime download (attempt {})", update.attempt)
        } else {
            "Downloading runtime".to_string()
        };
        progress(RuntimeInstallProgress::new(
            RuntimeInstallStep::Downloading,
            Some(0.05 + portion * 0.65),
            message,
        ));
    })
}

/// Downloads `url` beside `destination` and moves it into place once complete,
/// so `destination` never holds a partial file.
pub fn download_to(
    url: &str,
    destination: &Path,
    expected_size: Option<u64>,
    max_size: u64,
    progress: &mut impl FnMut(transfer::DownloadProgress),
) -> Result<(), RuntimeError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_extension("download");
    transfer::download_file(url, &temporary, expected_size, max_size, progress)?;
    crate::replace_file(&temporary, destination)?;
    Ok(())
}

/// Checks that `path` holds exactly `size` bytes whose SHA-1 is `expected`,
/// as CEF's build index lists them.
pub(crate) fn verify_with_progress(
    path: &Path,
    size: u64,
    expected: &str,
    progress: &mut impl FnMut(RuntimeInstallProgress),
) -> Result<(), RuntimeError> {
    let integrity_failed = || RuntimeError::IntegrityFailed {
        path: path.to_path_buf(),
    };
    if std::fs::metadata(path)?.len() != size {
        return Err(integrity_failed());
    }
    let mut read_total = 0_u64;
    let mut last_report = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let actual = crate::digest::digest_reader::<Sha1>(std::fs::File::open(path)?, |read| {
        read_total = read_total.saturating_add(read as u64);
        if last_report.elapsed() >= Duration::from_millis(100) {
            let portion = (read_total as f32 / size as f32).clamp(0.0, 1.0);
            progress(RuntimeInstallProgress::new(
                RuntimeInstallStep::Verifying,
                Some(0.72 + portion * 0.05),
                "Verifying runtime",
            ));
            last_report = Instant::now();
        }
    })?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(integrity_failed());
    }
    progress(RuntimeInstallProgress::new(
        RuntimeInstallStep::Verifying,
        Some(0.78),
        "Verifying runtime",
    ));
    Ok(())
}

pub(crate) fn first_extracted_runtime_dir(work_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(work_dir)
        .ok()?
        .flatten()
        .find_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            (path.is_dir() && name.starts_with("cef_binary_")).then_some(path)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_downloads_refuse_plain_http() {
        let destination = std::env::temp_dir().join(format!(
            "sabine-runtime-download-{}",
            crate::random_hex(8).unwrap()
        ));
        let error =
            download_file("http://127.0.0.1:9/runtime", &destination, 1, &mut |_| {}).unwrap_err();
        assert!(error.to_string().contains("not HTTPS"), "{error}");
        assert!(!destination.exists());
    }
}
