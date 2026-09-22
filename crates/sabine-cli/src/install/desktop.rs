use std::path::Path;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::process::Stdio;

#[cfg(target_os = "linux")]
use crate::commands::command_exists;
use crate::install::source::SourceApp;

pub fn install_entry(app: &SourceApp, executable: &Path) -> Result<(), String> {
    let icon = crate::icon_assets::install_user_icon(&app.id, app.icon.as_deref())?;
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
    install_windows_shortcut(app, executable, icon.as_deref())?;
    #[cfg(target_os = "macos")]
    install_macos_app(app, executable, icon.as_deref())?;
    Ok(())
}

pub fn link_macos_bundle(id: &str, bundle: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
        let directory = Path::new(&home).join("Applications");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let path = directory.join(format!("{id}.app"));
        if path.is_symlink() {
            std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        } else if path.exists() {
            return Err(format!(
                "{} already exists; uninstall it first",
                path.display()
            ));
        }
        std::os::unix::fs::symlink(bundle, path).map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (id, bundle);
        Err("macOS application bundles require macOS".into())
    }
}
#[cfg(target_os = "macos")]
use crate::macos_bundle::xml;

#[cfg(target_os = "linux")]
pub fn entry(app: &SourceApp, wrapper: &Path, desktop_icon: Option<&str>) -> String {
    let icon = desktop_icon.unwrap_or(&app.id);
    let mime_types = mime_type_line(&app.mime_types);
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec={} %U\nIcon={}\n{}Terminal=false\nCategories=Utility;\nStartupNotify=true\nStartupWMClass={}\n",
        desktop_value(&app.name),
        desktop_exec(wrapper),
        desktop_value(icon),
        mime_types,
        desktop_value(&app.id)
    )
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
        return std::fs::write(
            directory.join(format!("{}.desktop", app.id)),
            entry(app, wrapper, _desktop_icon),
        )
        .map_err(|error| error.to_string());
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
        return status
            .success()
            .then_some(())
            .ok_or_else(|| "failed to register Windows autostart entry".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set".to_string())?;
        let directory = Path::new(&home).join("Library/LaunchAgents");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let label = xml(&app.id);
        let executable = xml(&wrapper.display().to_string());
        return std::fs::write(
            directory.join(format!("{}.plist", app.id)),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>{label}</string><key>ProgramArguments</key><array><string>{executable}</string></array><key>RunAtLoad</key><true/></dict></plist>\n"
            ),
        )
        .map_err(|error| error.to_string());
    }
    #[allow(unreachable_code)]
    Err("autostart is unsupported on this platform".to_string())
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
    let script = format!(
        "$dir=[Environment]::GetFolderPath('Programs'); $shell=New-Object -ComObject WScript.Shell; $shortcut=$shell.CreateShortcut((Join-Path $dir '{name}.lnk')); $shortcut.TargetPath='{target}'; {icon}$shortcut.Save()"
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
        crate::macos_bundle::info_plist(
            &app.id,
            &app.name,
            &app.version,
            "launch",
            app.icon.is_some(),
            &app.mime_types,
        )?,
    )
    .map_err(|error| error.to_string())?;
    if let Some(icon) = &app.icon {
        let resources = contents.join("Resources");
        let icon_set = resources.join("icons");
        crate::icon_assets::stage_icon_set(&app.id, icon, &icon_set)?;
        crate::icon_assets::stage_macos_icon(&app.id, &icon_set, &resources.join("app.icns"))?;
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

#[cfg(target_os = "linux")]
fn mime_type_line(mime_types: &[String]) -> String {
    if mime_types.is_empty() {
        return String::new();
    }
    let values = mime_types
        .iter()
        .map(|mime_type| mime_type.trim())
        .filter(|mime_type| !mime_type.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        String::new()
    } else {
        format!("MimeType={};\n", values.join(";"))
    }
}

#[cfg(target_os = "linux")]
fn desktop_value(value: &str) -> String {
    value.replace(['\n', '\r'], " ")
}

#[cfg(target_os = "linux")]
fn desktop_exec(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    format!("\"{}\"", escaped.replace('\\', "\\\\"))
}

#[cfg(target_os = "windows")]
fn powershell_string(value: &str) -> String {
    value.replace('\'', "''").replace(['\r', '\n'], " ")
}
