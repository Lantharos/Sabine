use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{InstalledApp, SabineService, SabineVersion, ServiceResult};

#[cfg(not(target_os = "macos"))]
const MANIFEST: &str = "resources/Sabine.toml";
#[cfg(target_os = "macos")]
const MANIFEST: &str = "Contents/Resources/Sabine.toml";

#[derive(Deserialize)]
struct Manifest {
    app: ManifestApp,
}

#[derive(Deserialize)]
struct ManifestApp {
    id: String,
    name: String,
    sabine: Option<String>,
}

/// Every Sabine app on this machine. An app installed for every user counts
/// with the build on disk, which can be newer than the one it last registered.
pub(crate) fn installed_apps(service: &SabineService) -> ServiceResult<Vec<InstalledApp>> {
    let mut apps = service
        .apps()?
        .into_iter()
        .map(|app| {
            let app = InstalledApp {
                id: app.manifest.id,
                name: app.manifest.name,
                sabine: app.manifest.sabine,
                system: false,
            };
            (app.id.clone(), app)
        })
        .collect::<BTreeMap<_, _>>();
    apps.extend(system_apps().into_iter().map(|app| (app.id.clone(), app)));
    Ok(apps.into_values().collect())
}

/// The apps installed for every user, as their bundled manifests describe them.
fn system_apps() -> Vec<InstalledApp> {
    let Ok(entries) = fs::read_dir(system_apps_dir()) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| read_manifest(&entry.path().join(MANIFEST)))
        .collect()
}

fn read_manifest(path: &Path) -> Option<InstalledApp> {
    let manifest = toml::from_str::<Manifest>(&fs::read_to_string(path).ok()?).ok()?;
    Some(InstalledApp {
        sabine: SabineVersion::parse(&manifest.app.sabine?)?,
        id: manifest.app.id,
        name: manifest.app.name,
        system: true,
    })
}

#[cfg(target_os = "linux")]
fn system_apps_dir() -> PathBuf {
    sabine_runtime::system_dir()
}

#[cfg(target_os = "macos")]
fn system_apps_dir() -> PathBuf {
    PathBuf::from("/Applications")
}

#[cfg(windows)]
fn system_apps_dir() -> PathBuf {
    std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
}

/// The apps installed for every user that this build of Sabine does not run.
pub fn unsupported_system_apps() -> Vec<InstalledApp> {
    system_apps()
        .into_iter()
        .filter(|app| !crate::SystemCompatibility::current().accepts(app.sabine))
        .collect()
}
