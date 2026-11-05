mod launcher;
use launcher::launcher_script;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    desktop::{entry::Listing, icons, types::Associations},
    install::{assets::stage_icon, desktop as source_desktop},
    project::{CargoPackage, absolute_path, detect_icon, read_sabine_file, sanitize_id},
};

#[derive(Debug)]
pub struct InstallOptions {
    pub source: PathBuf,
    pub id: Option<String>,
    pub name: Option<String>,
    pub command: Option<String>,
    pub desktop: bool,
    pub autostart: bool,
}

#[derive(Clone, Debug)]
pub struct SourceApp {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: PathBuf,
    pub command: Option<String>,
    pub icon: Option<PathBuf>,
    pub associations: Associations,
    pub listing: Listing,
    pub autostart: bool,
}

pub fn install(options: InstallOptions) -> Result<ExitCode, String> {
    let mut app = detect_source_app(
        &options.source,
        options.id,
        options.name,
        options.command,
        options.autostart,
    )?;
    app.id = sabine_service::AppEnvironment::Development.app_id(&app.id);
    app.name = format!("{} (Development)", app.name);
    app.listing.listed = true;
    register_app(&app, options.desktop)?;
    println!("installed {} from {}", app.name, app.source.display());
    Ok(ExitCode::SUCCESS)
}

pub fn update(target: &str) -> Result<ExitCode, String> {
    let app = if Path::new(target).exists() {
        read_registered_app(&super::project_install_id(Path::new(target))?)?
    } else {
        read_registered_app(target)?
    };
    update_registered_app(&app)?;
    Ok(ExitCode::SUCCESS)
}

fn update_registered_app(app: &SourceApp) -> Result<(), String> {
    let mut app = detect_source_app(
        &app.source,
        Some(app.id.clone()),
        None,
        app.command.clone(),
        app.autostart,
    )?;
    app.name = format!("{} (Development)", app.name);
    app.listing.listed = true;
    register_app(&app, true)?;
    println!("updated {} from {}", app.name, app.source.display());
    Ok(())
}

pub(crate) fn detect_source_app(
    source: &Path,
    id: Option<String>,
    name: Option<String>,
    command: Option<String>,
    autostart: bool,
) -> Result<SourceApp, String> {
    let source = absolute_path(source)?;
    let sabine = read_sabine_file(&source)?;
    let listing = Listing::of(&sabine.app);
    listing.validate()?;
    let cargo = CargoPackage::read(&sabine.cargo_manifest(&source)).ok();
    let cargo_value = |field| cargo.as_ref().and_then(|cargo| cargo.string(field).ok());
    let version = sabine
        .app
        .version
        .or_else(|| cargo_value("version"))
        .unwrap_or_else(|| "0.1.0".into());
    let name = name
        .or(sabine.app.name)
        .or_else(|| cargo_value("name"))
        .ok_or_else(|| {
            "source app needs --name, app metadata, or Cargo.toml package metadata".to_string()
        })?;
    let id = id
        .or(sabine.app.id)
        .unwrap_or_else(|| format!("dev.sabine.{}", sanitize_id(&name)));
    Ok(SourceApp {
        id: sanitize_id(&id),
        name,
        version,
        command: command.or(sabine.install.command).or(sabine.app.command),
        icon: sabine
            .app
            .icon
            .map(|icon| source.join(icon))
            .or_else(|| detect_icon(&source)),
        source,
        associations: Associations::default(),
        listing,
        autostart,
    })
}

fn register_app(app: &SourceApp, desktop: bool) -> Result<(), String> {
    sabine_service::ensure_service_executable(|_| {}).map_err(|error| error.to_string())?;
    sabine_service::ensure_daemon_running().map_err(|error| error.to_string())?;
    let app_dir = app_dir(&app.id)?;
    fs::create_dir_all(&app_dir).map_err(|error| error.to_string())?;
    let staged_icon = stage_icon(&app_dir, app.icon.as_deref())?;
    let desktop_icon = icons::install_user_icon(&app.id, staged_icon.as_deref())?;
    let wrapper = app_dir.join(if cfg!(target_os = "windows") {
        "launch.cmd"
    } else {
        "launch.sh"
    });
    fs::write(&wrapper, launcher_script(app)?).map_err(|error| error.to_string())?;
    sabine_runtime::make_executable(&wrapper).map_err(|error| error.to_string())?;
    let record = toml::to_string(&SourceRecord::of(app)).map_err(|error| error.to_string())?;
    fs::write(app_dir.join(SOURCE_RECORD), record).map_err(|error| error.to_string())?;

    sabine_service::SabineService::default()
        .register(sabine_service::AppManifest {
            id: app.id.clone(),
            name: app.name.clone(),
            version: app.version.clone(),
            executable: wrapper.clone(),
            args: Vec::new(),
            update: None,
            sabine: sabine_service::SabineVersion::current(),
        })
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "linux")]
    if desktop {
        let desktop_dir = applications_dir()?;
        fs::create_dir_all(&desktop_dir).map_err(|error| error.to_string())?;
        fs::write(
            desktop_dir.join(format!("{}.desktop", app.id)),
            source_desktop::entry(app, &wrapper, desktop_icon.as_deref()),
        )
        .map_err(|error| error.to_string())?;
        sabine_service::refresh_desktop_database(&desktop_dir);
    }
    #[cfg(target_os = "windows")]
    if desktop {
        source_desktop::install_windows_shortcut(
            &app.name,
            app.listing.generic_name.as_deref(),
            &wrapper,
            desktop_icon.as_deref(),
            false,
        )?;
    }
    #[cfg(target_os = "macos")]
    if desktop {
        source_desktop::install_macos_app(app, &wrapper)?;
    }
    if app.autostart {
        source_desktop::install_autostart(app, &wrapper, desktop_icon.as_deref())?;
    }
    Ok(())
}

const SOURCE_RECORD: &str = "source-install.toml";

/// What `sabine install` remembers of a checkout to update its launcher from.
#[derive(Deserialize, Serialize)]
struct SourceRecord {
    id: String,
    name: String,
    version: String,
    source: PathBuf,
    command: Option<String>,
    icon: Option<PathBuf>,
    autostart: bool,
}

impl SourceRecord {
    fn of(app: &SourceApp) -> Self {
        Self {
            id: app.id.clone(),
            name: app.name.clone(),
            version: app.version.clone(),
            source: app.source.clone(),
            command: app.command.clone(),
            icon: app.icon.clone(),
            autostart: app.autostart,
        }
    }
}

pub(crate) fn read_registered_app(id: &str) -> Result<SourceApp, String> {
    let path = app_dir(&sanitize_id(id))?.join(SOURCE_RECORD);
    let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let record = toml::from_str::<SourceRecord>(&text)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    Ok(SourceApp {
        id: record.id,
        name: record.name,
        version: record.version,
        source: record.source,
        command: record.command,
        icon: record.icon,
        associations: Associations::default(),
        listing: Listing::default(),
        autostart: record.autostart,
    })
}

pub(crate) fn apps_root() -> Result<PathBuf, String> {
    Ok(sabine_runtime::sabine_data_dir().join("apps"))
}

pub(crate) fn app_dir(id: &str) -> Result<PathBuf, String> {
    Ok(apps_root()?.join(id))
}

#[cfg(target_os = "linux")]
fn applications_dir() -> Result<PathBuf, String> {
    Ok(data_home()?.join("applications"))
}

#[cfg(target_os = "linux")]
pub(crate) fn data_home() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(path));
    }
    home_dir()
        .map(|home| home.join(".local/share"))
        .ok_or_else(|| "HOME is not set".to_string())
}

#[cfg(target_os = "linux")]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
