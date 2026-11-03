use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use sabine_service::manifest::SabineFile;

use super::SabineWindow;
use crate::error::{SabineError, SabineResult};

const MANIFEST_ENV: &str = "SABINE_MANIFEST_PATH";
const APP_ID_ENV: &str = "SABINE_APP_ID";
const SCHEME_HANDLER: &str = "x-scheme-handler/";

impl SabineWindow {
    /// Creates another window using the current app's resolved identity and
    /// web entry. The launched process supplies the primary window's bridge.
    pub fn for_current_app() -> SabineResult<Self> {
        Self::new().with_framework_config()
    }

    /// Applies `[app]`, production `[web]` and `[window]` fields from a
    /// `Sabine.toml`.
    ///
    /// Bridge handlers, chrome, and size stay in Rust; identity and content
    /// paths can live in the manifest shared with the CLI/bundler.
    pub fn with_manifest(mut self, path: impl AsRef<Path>) -> SabineResult<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| SabineError::ManifestRead {
            path: path.to_path_buf(),
            source,
        })?;
        let (file, unknown) =
            SabineFile::parse(&text).map_err(|source| SabineError::ManifestParse {
                path: path.to_path_buf(),
                source,
            })?;
        if !unknown.is_empty() {
            sabine_runtime::report_error(
                "manifest",
                format!(
                    "{} has keys Sabine doesn't know, which do nothing: {}",
                    path.display(),
                    unknown.join(", ")
                ),
            );
        }
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        if base.join("runtimes/cef").is_dir() {
            self.config.runtime.bundled_dir = Some(base.to_path_buf());
        }
        #[cfg(target_os = "linux")]
        if file.app.shared_runtime == Some(false) {
            self = self.shared_runtime(false);
        }
        if let Some(id) = file.app.id {
            self = self.app_id(id);
        }
        if let Some(name) = file.app.name {
            self = self.title(name);
        }
        if let Some(version) = file.app.version {
            self = self.app_version(version);
        }
        let schemes = file
            .app
            .mime_types
            .iter()
            .filter_map(|mime| mime.strip_prefix(SCHEME_HANDLER))
            .collect::<Vec<_>>();
        if !schemes.is_empty() {
            let id = self
                .config
                .app_id
                .clone()
                .ok_or_else(|| SabineError::CreationFailed {
                    message: "URL handlers require an app id".into(),
                })?;
            self = self.deep_link(sabine_platform::DeepLinkRegistration::new(id, schemes));
        }
        self.config.documents = super::DocumentTypes {
            #[cfg(target_os = "linux")]
            types: file
                .app
                .mime_types
                .iter()
                .filter(|mime| !mime.starts_with(SCHEME_HANDLER))
                .cloned()
                .collect(),
            extensions: file.app.extensions.into_values().flatten().collect(),
        };
        self.config.app_update = file
            .updates
            .map(sabine_service::AppUpdatesSection::resolve)
            .transpose()
            .map_err(|message| SabineError::CreationFailed { message })?
            .flatten();
        #[cfg(target_os = "linux")]
        {
            self.config.sandbox = file
                .sandbox
                .map(sabine_service::SandboxSection::resolve)
                .transpose()
                .map_err(|message| SabineError::CreationFailed { message })?;
        }
        if let Some(entry) = file.web.entry {
            let entry_path = base.join(&entry);
            self = self.entry(entry_path.display().to_string());
        }
        if let Some(url) = file.web.url {
            self = self.url(url);
        }

        if file.window.splash {
            self = match file.window.splash_delay_ms {
                Some(delay) => self.splash_after(std::time::Duration::from_millis(delay)),
                None => self.splash(true),
            };
        }
        self = self.local_files(file.web.local_files);
        for origin in file.web.allowed_origins {
            self = self.allowed_origin(origin);
        }
        Ok(self)
    }

    pub(super) fn with_framework_config(mut self) -> SabineResult<Self> {
        if let Some(path) = framework_manifest_path() {
            self = self.with_manifest(path)?;
        }
        if let Some(app_id) = development_env(APP_ID_ENV) {
            self = self.app_id(app_id);
        }
        Ok(self)
    }
}

/// `[resources]` from the app's `Sabine.toml`, with the folder its paths
/// start from.
struct Resources {
    base: PathBuf,
    paths: BTreeMap<String, String>,
}

/// A file or folder that `[resources]` in `Sabine.toml` names: in the source
/// project during `sabine dev` and `cargo run`, and in the bundle once the app
/// is packaged.
pub fn resource_path(name: &str) -> Option<PathBuf> {
    let resources = resources()?;
    let path = resources.base.join(resources.paths.get(name)?);
    path.exists().then_some(path)
}

fn resources() -> Option<&'static Resources> {
    static RESOURCES: OnceLock<Option<Resources>> = OnceLock::new();
    RESOURCES.get_or_init(read_resources).as_ref()
}

fn read_resources() -> Option<Resources> {
    let manifest = framework_manifest_path()?;
    let file = fs::read_to_string(&manifest)
        .map_err(|error| error.to_string())
        .and_then(|text| SabineFile::parse(&text).map_err(|error| error.to_string()));
    match file {
        Ok((file, _)) => Some(Resources {
            base: manifest.parent()?.to_path_buf(),
            paths: file.resources,
        }),
        Err(error) => {
            sabine_runtime::report_error(
                "manifest",
                format!(
                    "could not read [resources] from {}: {error}",
                    manifest.display()
                ),
            );
            None
        }
    }
}

fn framework_manifest_path() -> Option<PathBuf> {
    if let Some(path) = development_env(MANIFEST_ENV) {
        return Some(PathBuf::from(path));
    }

    if let Ok(executable) = env::current_exe()
        && let Some(directory) = executable.parent()
    {
        let candidates = [
            directory.join("resources/Sabine.toml"),
            directory.join("../Resources/Sabine.toml"),
        ];
        if let Some(path) = candidates.into_iter().find(|path| path.is_file()) {
            return Some(path);
        }
    }

    if crate::AppEnvironment::production_build() {
        return None;
    }
    let cargo_package = env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from);
    [env::current_dir().ok(), cargo_package]
        .into_iter()
        .flatten()
        .map(|directory| directory.join("Sabine.toml"))
        .find(|path| path.is_file())
}

/// A variable that points a development run at its project. Production
/// builds read their identity and pages only from their bundle.
pub(super) fn development_env(name: &str) -> Option<String> {
    if crate::AppEnvironment::production_build() {
        return None;
    }
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
