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

    /// The environment an app id was registered from.
    pub fn of(app_id: &str) -> Self {
        if app_id.ends_with(".dev") {
            Self::Development
        } else {
            Self::Production
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
            Self::Development if Self::of(id) == Self::Production => format!("{id}.dev"),
            _ => id.to_owned(),
        }
    }

    /// The folder an app keeps its files in: its app id below the desktop's
    /// data folder (`%APPDATA%`, `~/Library/Application Support` or
    /// `$XDG_DATA_HOME`), which Sabine's sandbox lets the app write. Development
    /// runs use the `.dev` app id.
    pub fn app_data_dir(self, app_id: &str) -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(self.app_id(app_id))
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
