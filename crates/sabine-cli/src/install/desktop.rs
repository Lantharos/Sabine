use std::path::Path;
use std::process::Command;
#[cfg(target_os = "linux")]
use std::process::Stdio;

#[cfg(target_os = "linux")]
use crate::commands::command_exists;
use crate::install::source::SourceApp;

pub fn install_entry(app: &SourceApp, executable: &Path) -> Result<(), String> {
    let icon = crate::desktop::icons::install_user_icon(&app.id, app.icon.as_deref())?;
    #[cfg(target_os = "linux")]
    {
        let directory = crate::install::source::data_home()?.join("applications");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        std::fs::write(
            directory.join(format!("{}.desktop", app.id)),
            entry(app, executable, icon.as_deref()),
        )
        .map_err(|error| error.to_string())?;
        refresh_database(&directory);
    }
    #[cfg(target_os = "windows")]
    {
        register_windows_schemes(app, executable)?;
        if app.listing.listed {
            install_windows_shortcut(app, executable, icon.as_deref())?;
        } else {
            remove_windows_shortcut(&app.id)?;
        }
    }
    #[cfg(target_os = "macos")]
    install_macos_app(app, executable, icon.as_deref())?;
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
        icon: Some(desktop_icon.unwrap_or(&app.id)),
        mime_types: &app.mime_types,
        listing: &app.listing,
    }
    .render()
}

#[cfg(target_os = "linux")]
pub fn refresh_database(applications_dir: &Path) {
    if !command_exists("update-desktop-database") {
        return;
    }
    let _ = Command::new("update-desktop-database")
        .arg(applications_dir)
        .stdin(Stdio::null())
        .status();
}

pub fn install_autostart(
    app: &SourceApp,
    wrapper: &Path,
    _desktop_icon: Option<&str>,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let directory = crate::install::source::autostart_dir()?;
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        std::fs::write(
            directory.join(format!("{}.desktop", app.id)),
            entry(app, wrapper, _desktop_icon),
        )
        .map_err(|error| error.to_string())
    }
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                &app.id,
                "/t",
                "REG_SZ",
                "/d",
                &format!("\"{}\"", wrapper.display()),
                "/f",
            ])
            .status()
            .map_err(|error| error.to_string())?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| "failed to register Windows autostart entry".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
        let directory = Path::new(&home).join("Library/LaunchAgents");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let label = xml(&app.id);
        let executable = xml(&wrapper.display().to_string());
        std::fs::write(
            directory.join(format!("{}.plist", app.id)),
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
    _desktop_icon: Option<&str>,
) -> Result<(), String> {
    let name = powershell_string(&app.id);
    let target = powershell_string(&wrapper.display().to_string());
    let icon = _desktop_icon
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
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|error| error.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "failed to create Windows Start Menu shortcut".to_string())
}

#[cfg(target_os = "macos")]
pub fn install_macos_app(
    app: &SourceApp,
    wrapper: &Path,
    _desktop_icon: Option<&str>,
) -> Result<(), String> {
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
            &app.mime_types,
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
fn register_windows_schemes(app: &SourceApp, executable: &Path) -> Result<(), String> {
    let command = format!("\"{}\" \"%1\"", executable.display());
    for scheme in crate::desktop::types::schemes(&app.mime_types) {
        let key = format!(r"HKCU\Software\Classes\{scheme}");
        set_registry_string(&key, None, &format!("URL:{scheme}"))?;
        set_registry_string(&key, Some("URL Protocol"), "")?;
        set_registry_string(&format!(r"{key}\shell\open\command"), None, &command)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn set_registry_string(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let mut command = Command::new("reg");
    command.args(["add", key]);
    match name {
        Some(name) => command.args(["/v", name]),
        None => command.arg("/ve"),
    };
    let status = command
        .args(["/t", "REG_SZ", "/d", value, "/f"])
        .status()
        .map_err(|error| error.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("failed to register {key}"))
}

#[cfg(target_os = "windows")]
fn powershell_string(value: &str) -> String {
    value.replace('\'', "''").replace(['\r', '\n'], " ")
}
