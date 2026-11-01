use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use sabine_service::{Sandbox, SandboxSection, manifest::WindowSection};

use super::file::read_sabine_file;
use super::updates::resolve_updates;
use super::web::{WebBundle, resolve_web};
use super::{CargoPackage, absolute_path, sanitize_id};
use crate::desktop::{entry::Listing, types::Associations};

#[derive(Debug)]
pub(crate) struct BundleApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub maintainer: Option<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub description: Option<String>,
    pub icon: Option<PathBuf>,
    pub installer_image: Option<PathBuf>,
    pub associations: Associations,
    pub listing: Listing,
    pub cargo_manifest: PathBuf,
    pub source_dir: PathBuf,
    pub cargo_package: String,
    pub web: Option<WebBundle>,
    pub updates: Option<sabine_service::AppUpdateConfig>,
    pub window: WindowSection,
    pub shared_runtime: bool,
    pub sandbox: Option<Sandbox>,
    /// Files and folders shipped beside the app, by the name it finds them with.
    pub resources: Vec<(String, PathBuf)>,
}

#[derive(Debug, Default)]
pub(crate) struct ConfigOverrides {
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub web_build: Option<String>,
    pub web_root: Option<PathBuf>,
    pub web_dist: Option<PathBuf>,
}

pub(crate) fn resolve_app(source: &Path, overrides: ConfigOverrides) -> Result<BundleApp, String> {
    let source_dir = absolute_path(source)?;
    let sabine = read_sabine_file(&source_dir)?;
    let listing = Listing::of(&sabine.app);
    let cargo_manifest = sabine.cargo_manifest(&source_dir);
    let updates = resolve_updates(sabine.updates)?;
    let sandbox = sabine.sandbox.map(SandboxSection::resolve).transpose()?;
    let resources = resolve_resources(&source_dir, &sabine.resources)?;
    let cargo = CargoPackage::read(&cargo_manifest)?;
    let cargo_package = cargo.string("name")?;
    let web = resolve_web(&source_dir, &sabine.web, &overrides)?;

    let name = overrides
        .name
        .or(sabine.app.name)
        .unwrap_or_else(|| cargo_package.replace('_', " "));
    let id = overrides
        .id
        .or(sabine.app.id)
        .unwrap_or_else(|| format!("dev.sabine.{}", sanitize_id(&name)));
    let version = match overrides.version.or(sabine.app.version) {
        Some(version) => version,
        None => cargo.string("version")?,
    };
    let authors = cargo.strings("authors");
    let maintainer = sabine.app.maintainer.or_else(|| {
        authors
            .iter()
            .find(|author| author.contains('<') && author.contains('@'))
            .cloned()
    });
    let publisher = sabine
        .app
        .publisher
        .or_else(|| {
            authors
                .first()
                .map(|author| author.split('<').next().unwrap_or(author).trim().to_owned())
        })
        .unwrap_or_else(|| name.clone());
    let license = sabine.app.license.or_else(|| cargo.string("license").ok());
    let homepage = sabine
        .app
        .homepage
        .or_else(|| cargo.string("homepage").ok())
        .or_else(|| cargo.string("repository").ok());
    for (field, value) in [
        ("publisher", Some(publisher.as_str())),
        ("maintainer", maintainer.as_deref()),
        ("license", license.as_deref()),
        ("homepage", homepage.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().is_empty() || value.chars().any(char::is_control))
        {
            return Err(format!(
                "app {field} must be nonempty and contain no control characters"
            ));
        }
    }
    let icon = sabine
        .app
        .icon
        .map(|icon| source_dir.join(icon))
        .or_else(|| detect_icon(&source_dir));

    if let Some(icon) = &icon
        && !icon.is_file()
    {
        return Err(format!("app icon was not found: {}", icon.display()));
    }
    let installer_image = sabine
        .app
        .installer_image
        .map(|image| source_dir.join(image));
    if let Some(image) = &installer_image
        && !image.is_file()
    {
        return Err(format!(
            "installer image was not found: {}",
            image.display()
        ));
    }
    if !sabine_service::valid_app_id(&id) {
        return Err("app id must contain lowercase letters, digits, dots or hyphens and must not be a relative path".to_string());
    }
    for (field, value) in [("name", &name), ("version", &version)] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(format!(
                "app {field} must be nonempty and contain no control characters"
            ));
        }
    }
    semver::Version::parse(&version).map_err(|error| format!("invalid app version: {error}"))?;
    listing.validate()?;
    let associations = Associations {
        mime_types: sabine.app.mime_types,
        extensions: sabine.app.extensions,
    };
    associations.validate()?;
    Ok(BundleApp {
        id,
        name,
        version,
        publisher,
        maintainer,
        license,
        homepage,
        description: sabine.app.description,
        icon,
        installer_image,
        associations,
        listing,
        cargo_manifest,
        source_dir,
        cargo_package,
        web,
        updates,
        window: sabine.window,
        shared_runtime: sabine.app.shared_runtime.unwrap_or(true),
        sandbox,
        resources,
    })
}

/// `[resources]`: names the app looks files and folders up by, each with
/// its path from `Sabine.toml`.
fn resolve_resources(
    source_dir: &Path,
    resources: &BTreeMap<String, String>,
) -> Result<Vec<(String, PathBuf)>, String> {
    resources
        .iter()
        .map(|(name, path)| {
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
            {
                return Err(format!(
                    "resource name {name:?} may contain only lowercase letters, digits, hyphens and underscores"
                ));
            }
            let path = source_dir.join(path);
            if !path.exists() {
                return Err(format!("resource {name} was not found at {}", path.display()));
            }
            Ok((name.clone(), path))
        })
        .collect()
}

pub(crate) fn detect_icon(source_dir: &Path) -> Option<PathBuf> {
    [
        "static/icon.svg",
        "static/favicon.svg",
        "src/lib/assets/favicon.svg",
        "favicon.svg",
        "icon.svg",
        "icon.png",
        "icons/icon.svg",
        "icons/icon.png",
        "desktop/icons/icon.svg",
        "static/icon.png",
        "desktop/icons/icon.png",
    ]
    .iter()
    .map(|icon| source_dir.join(icon))
    .find(|icon| icon.is_file())
}
