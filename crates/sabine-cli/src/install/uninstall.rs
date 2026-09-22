use sabine_service::SabineService;
#[cfg(any(windows, target_os = "macos"))]
use std::path::PathBuf;
use std::{fs, path::Path, process::ExitCode, time::Duration};

pub fn run(target: Option<String>, system: bool, purge: bool) -> Result<ExitCode, String> {
    if system {
        let removed_self =
            sabine_service::uninstall_system(purge).map_err(|error| error.to_string())?;
        println!(
            "Removed Sabine components and CLI{}",
            if purge {
                " and Sabine data"
            } else {
                "; app data was kept"
            }
        );
        if !removed_self {
            self_replace::self_delete().map_err(|error| error.to_string())?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    let target = target.unwrap_or_else(|| ".".into());
    let id = if Path::new(&target).is_dir() {
        super::project_install_id(Path::new(&target))?
    } else {
        target
    };
    if !sabine_service::valid_app_id(&id) {
        return Err("invalid app identifier".into());
    }
    let service = SabineService::default();
    let app = service.app(&id).map_err(|error| error.to_string())?;
    let directory = service.root().join("apps").join(&id);
    let source_install = directory.join("source-install.toml").is_file();
    let bundle_install = directory.join("bundle-install.json").is_file();
    if !source_install && !bundle_install {
        return Err(format!(
            "{} was not installed by sabine install; remove it through its package manager or OS uninstaller",
            app.manifest.name
        ));
    }
    let lock = sabine_runtime::FileLock::acquire(
        &directory.join("update.lock"),
        Duration::from_secs(30),
        |_| {},
    )
    .map_err(|error| error.to_string())?;
    remove_desktop(&id)?;
    if bundle_install && !purge {
        let payload = if cfg!(target_os = "macos") {
            directory.join("install").join(format!("{id}.app"))
        } else {
            directory.join("install")
        };
        sabine_service::remove_app_payload(&payload, &id).map_err(|error| error.to_string())?;
    } else if purge {
        remove_path(&directory.join("install"))?;
    }
    for name in ["releases", "launch.sh", "launch.cmd", "web", "icons"] {
        remove_path(&directory.join(name))?;
    }
    remove_path(&service.root().join("downloads").join(&id))?;
    service.unregister(&id).map_err(|error| error.to_string())?;
    for name in [
        "source-install.toml",
        "bundle-install.json",
        "pending-update.json",
    ] {
        remove_path(&directory.join(name))?;
    }
    drop(lock);
    remove_path(&directory.join("update.lock"))?;
    if purge {
        remove_path(&directory)?;
        let profile = sabine_service::browser_profile_path(&id);
        remove_path(profile.parent().ok_or("browser profile has no parent")?)?;
    }
    println!(
        "Uninstalled {}{}",
        app.manifest.name,
        if purge {
            " and its Sabine data"
        } else {
            "; app data was kept"
        }
    );
    Ok(ExitCode::SUCCESS)
}

fn remove_desktop(id: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let data = super::source::data_home()?;
        let applications = data.join("applications");
        remove_path(&applications.join(format!("{id}.desktop")))?;
        remove_path(&super::source::autostart_dir()?.join(format!("{id}.desktop")))?;
        let icons = data.join("icons/hicolor");
        if icons.is_dir() {
            for entry in fs::read_dir(&icons).map_err(|error| error.to_string())? {
                let size = entry.map_err(|error| error.to_string())?.path();
                if size.is_symlink() || !size.is_dir() {
                    continue;
                }
                for extension in ["png", "svg"] {
                    remove_path(&size.join("apps").join(format!("{id}.{extension}")))?;
                }
            }
        }
        super::desktop::refresh_database(&applications);
    }
    #[cfg(target_os = "macos")]
    {
        let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
        let agent = home
            .join("Library/LaunchAgents")
            .join(format!("{id}.plist"));
        if agent.is_file() {
            let _ = std::process::Command::new("launchctl")
                .arg("bootout")
                .arg(&agent)
                .status();
            remove_path(&agent)?;
        }
        remove_path(&home.join("Applications").join(format!("{id}.app")))?;
    }
    #[cfg(windows)]
    {
        let roaming = PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA is not set")?);
        remove_path(
            &roaming
                .join("Microsoft/Windows/Start Menu/Programs")
                .join(format!("{id}.lnk")),
        )?;
        let _ = sabine_runtime::background_command("reg.exe")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                id,
                "/f",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    Ok(())
}

fn remove_path(path: &Path) -> Result<(), String> {
    let result = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => Err(error),
    };
    result.map_err(|error| format!("could not remove {}: {error}", path.display()))
}
