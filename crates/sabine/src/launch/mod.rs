pub(crate) mod bootstrap;
pub(crate) mod browser;
#[cfg(target_os = "linux")]
pub(crate) mod confine;
pub(crate) mod dev_server;
pub(crate) mod executable;
pub(crate) mod metrics;
#[cfg(target_os = "linux")]
pub(crate) mod sandbox;
#[cfg(target_os = "linux")]
pub(crate) mod shared;

use std::path::PathBuf;

use crate::error::{SabineError, SabineResult};
use crate::osr;
use crate::window::config::SabineWindowConfig;

pub(crate) fn dispatch_host_mode_from_args(args: &[String]) -> bool {
    if osr::launch::starts_window_host(args) {
        osr::launch::run_window_host();
        return true;
    }
    if args.iter().any(|arg| {
        matches!(
            arg.as_str(),
            bootstrap::BOOTSTRAP_ARG
                | bootstrap::NOTICE_ARG
                | bootstrap::CONFIRM_UPDATE_ARG
                | bootstrap::BLOCKED_ARG
        )
    }) {
        return bootstrap::run_from_args(args);
    }
    false
}

pub(crate) fn metrics_label(config: &SabineWindowConfig) -> String {
    config
        .app_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&config.title)
        .to_string()
}

pub(crate) const APP_URL_PREFIX: &str = "sabine://app/";

/// A local web entry, served to Chromium from its directory at `sabine://app/`.
pub(crate) struct LocalEntry {
    pub(crate) root: PathBuf,
    pub(crate) url: String,
}

pub(crate) fn local_entry(entry: &str) -> SabineResult<LocalEntry> {
    let (entry_path, suffix) = split_entry_suffix(entry);
    let path = canonical_entry(entry_path)?;
    let (Some(root), Some(file)) = (path.parent(), path.file_name()) else {
        return Err(SabineError::CreationFailed {
            message: format!("CEF entry is not a file: {}", path.display()),
        });
    };
    let mut url = url::Url::parse(APP_URL_PREFIX).expect("the app URL prefix is valid");
    url.path_segments_mut()
        .expect("the app URL has a path")
        .pop_if_empty()
        .push(&file.to_string_lossy());
    Ok(LocalEntry {
        root: root.to_path_buf(),
        url: format!("{url}{suffix}"),
    })
}

fn canonical_entry(entry: &str) -> SabineResult<PathBuf> {
    if entry.trim().is_empty() {
        return Err(SabineError::CreationFailed {
            message: "CEF entry path is empty".to_string(),
        });
    }
    let entry_path = PathBuf::from(entry);
    let path = if entry_path.is_absolute() {
        entry_path
    } else {
        std::env::current_dir()
            .map_err(|error| SabineError::CreationFailed {
                message: error.to_string(),
            })?
            .join(entry_path)
    };
    path.canonicalize()
        .map_err(|error| SabineError::CreationFailed {
            message: format!("failed to resolve CEF entry: {error}"),
        })
}

fn split_entry_suffix(entry: &str) -> (&str, &str) {
    let split = [entry.find('?'), entry.find('#')]
        .into_iter()
        .flatten()
        .min();
    match split {
        Some(index) => (&entry[..index], &entry[index..]),
        None => (entry, ""),
    }
}
