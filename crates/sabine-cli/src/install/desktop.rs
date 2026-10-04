use std::path::Path;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::process::Stdio;

use crate::install::source::SourceApp;

pub fn install_entry(app: &SourceApp, executable: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let icon = crate::desktop::icons::install_user_icon(&app.id, app.icon.as_deref())?;
        let data = crate::install::source::data_home()?;
        let directory = data.join("applications");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        std::fs::write(
            directory.join(format!("{}.desktop", app.id)),
            entry(app, executable, icon.as_deref()),
        )
        .map_err(|error| error.to_string())?;
        if let Some(package) = crate::desktop::mime_package::mime_package(&app.associations) {
            let packages = data.join("mime/packages");
            std::fs::create_dir_all(&packages).map_err(|error| error.to_string())?;
            std::fs::write(packages.join(format!("{}.xml", app.id)), package)
                .map_err(|error| error.to_string())?;
            refresh_mime_database(&data.join("mime"));
        }
        refresh_database(&directory);
    }
    #[cfg(target_os = "windows")]
    {
        let icon = crate::desktop::icons::install_user_icon(&app.id, app.icon.as_deref())?;
        super::handlers::windows::register(app, executable)?;
        if app.listing.listed {
            install_windows_shortcut(app, executable, icon.as_deref())?;
        } else {
            remove_windows_shortcut(&app.id)?;
        }
    }
    #[cfg(target_os = "macos")]
    install_macos_app(app, executable)?;
    Ok(())
}

pub fn install_macos_bundle(id: &str, bundle: &Path, listed: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        let directory = Path::new(&home).join("Applications");
        let path = directory.join(format!("{id}.app"));
        if path.is_symlink() {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        } else if path.exists() {
            return Err(format!(
                "{} already exists; uninstall it first",
                path.display()
            ));
        }
        if !listed {
            return register_with_launch_services(bundle);
        }
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        std::os::unix::fs::symlink(bundle, path).map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (id, bundle, listed);
        Err("macOS application bundles require macOS".into())
    }
}

#[cfg(target_os = "macos")]
pub(crate) const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";

#[cfg(target_os = "macos")]
fn register_with_launch_services(bundle: &Path) -> Result<(), String> {
    let status = Command::new(LSREGISTER)
        .arg("-f")
        .arg(bundle)
        .status()
        .map_err(|error| error.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "failed to register the app with Launch Services".to_string())
}

#[cfg(target_os = "macos")]
use crate::desktop::macos::xml;

#[cfg(target_os = "linux")]
pub fn entry(app: &SourceApp, wrapper: &Path, desktop_icon: Option<&str>) -> String {
    crate::desktop::entry::Entry {
        id: &app.id,
        name: &app.name,
        exec: &wrapper.to_string_lossy(),
        icon: desktop_icon,
        mime_types: &app.associations.mime_types,
        listing: &app.listing,
    }
    .render()
}

#[cfg(target_os = "linux")]
pub fn refresh_database(applications_dir: &Path) {
    refresh("update-desktop-database", applications_dir);
}

#[cfg(target_os = "linux")]
pub fn refresh_mime_database(mime_dir: &Path) {
    refresh("update-mime-database", mime_dir);
}

#[cfg(target_os = "linux")]
fn refresh(tool: &str, directory: &Path) {
    if sabine_runtime::find_program(tool).is_none() {
        return;
    }
    let _ = Command::new(tool)
        .arg(directory)
        .stdin(Stdio::null())
        .status();
}

pub fn install_autostart(
    app: &SourceApp,
    wrapper: &Path,
    desktop_icon: Option<&str>,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let path =
            sabine_service::app_autostart_path(&app.id).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(path.parent().expect("the autostart folder has a parent"))
            .map_err(|error| error.to_string())?;
        std::fs::write(path, entry(app, wrapper, desktop_icon)).map_err(|error| error.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        let _ = desktop_icon;
        sabine_service::windows_registry::set_current_user_value(
            sabine_service::APP_AUTOSTART_KEY,
            &app.id,
            &format!("\"{}\"", wrapper.display()),
        )
        .map_err(|error| error.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = desktop_icon;
        let path =
            sabine_service::app_autostart_path(&app.id).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(path.parent().expect("LaunchAgents has a parent"))
            .map_err(|error| error.to_string())?;
        let label = xml(&sabine_service::app_autostart_label(&app.id));
        let executable = xml(&wrapper.display().to_string());
        std::fs::write(
            path,
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{label}</string><key>ProgramArguments</key><array><string>{executable}</string></array><key>RunAtLoad</key><true/></dict></plist>\n"
            ),
        )
        .map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "windows")]
pub fn install_windows_shortcut(
    app: &SourceApp,
    wrapper: &Path,
    icon: Option<&str>,
) -> Result<(), String> {
    let name = powershell_string(&app.id);
    let target = powershell_string(&wrapper.display().to_string());
    let icon = icon
        .map(|icon| format!("$shortcut.IconLocation='{}';", powershell_string(icon)))
        .unwrap_or_default();
    let description = app
        .listing
        .generic_name
        .as_deref()
        .map(|name| format!("$shortcut.Description='{}';", powershell_string(name)))
        .unwrap_or_default();
    let script = format!(
        "$dir=[Environment]::GetFolderPath('Programs'); $shell=New-Object -ComObject WScript.Shell; $shortcut=$shell.CreateShortcut((Join-Path $dir '{name}.lnk')); $shortcut.TargetPath='{target}'; {icon}{description}$shortcut.Save()"
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|error| error.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "failed to create Windows Start Menu shortcut".to_string())
}

#[cfg(target_os = "macos")]
pub fn install_macos_app(app: &SourceApp, wrapper: &Path) -> Result<(), String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
    let root = Path::new(&home)
        .join("Applications")
        .join(format!("{}.app", app.id));
    let contents = root.join("Contents");
    let macos = contents.join("MacOS");
    std::fs::create_dir_all(&macos).map_err(|error| error.to_string())?;
    std::fs::write(
        contents.join("Info.plist"),
        crate::desktop::macos::info_plist(
            &app.id,
            &app.name,
            &app.version,
            "launch",
            app.icon.is_some(),
            &app.associations,
            &app.listing,
        )?,
    )
    .map_err(|error| error.to_string())?;
    if let Some(icon) = &app.icon {
        let resources = contents.join("Resources");
        let icon_set = resources.join("icons");
        crate::desktop::icons::stage_icon_set(&app.id, icon, &icon_set)?;
        crate::desktop::icons::stage_macos_icon(&app.id, &icon_set, &resources.join("app.icns"))?;
    }
    let launch = macos.join("launch");
    std::fs::write(
        &launch,
        format!(
            "#!/bin/sh\nexec '{}' \"$@\"\n",
            wrapper.display().to_string().replace('\'', "'\\''")
        ),
    )
    .map_err(|error| error.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(&launch)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(launch, permissions).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn remove_windows_shortcut(id: &str) -> Result<(), String> {
    let programs =
        std::path::PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA is not set")?)
            .join("Microsoft/Windows/Start Menu/Programs");
    match std::fs::remove_file(programs.join(format!("{id}.lnk"))) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

#[cfg(target_os = "windows")]
fn powershell_string(value: &str) -> String {
    value.replace('\'', "''").replace(['\r', '\n'], " ")
}
