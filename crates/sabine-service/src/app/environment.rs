use std::path::{Path, PathBuf};

/// Selects development isolation independently of Rust's optimization profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppEnvironment {
    Development,
    Production,
}

impl AppEnvironment {
    pub fn current() -> Self {
        match std::env::var("SABINE_ENV").as_deref() {
            Ok("development") => Self::Development,
            Ok("production") => Self::Production,
            _ if std::env::var("SABINE_DEV_URL").is_ok_and(|url| !url.trim().is_empty()) => {
                Self::Development
            }
            _ => Self::Production,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }

    /// Development apps have distinct registration, browser storage and desktop identities.
    pub fn app_id(self, id: &str) -> String {
        match self {
            Self::Development if !id.ends_with(".dev") => format!("{id}.dev"),
            _ => id.to_owned(),
        }
    }

    /// Keeps an existing production data location and isolates development below it.
    /// This computes the path without creating directories or moving existing data.
    pub fn data_dir(self, production: impl AsRef<Path>) -> PathBuf {
        match self {
            Self::Development => production.as_ref().join("development"),
            Self::Production => production.as_ref().to_path_buf(),
        }
    }
}
