use sabine_service::SabineService;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::{fs, path::Path, process::ExitCode};

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
        if let Some(path) = crate::handover::system_owned_entry() {
            println!("{} stays, since the system owns it", path.display());
        } else if !removed_self {
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
    let directory = service.root().join("apps").join(&id);
    let source_install = directory.join("source-install.toml").is_file();
    let bundle_install = directory.join("bundle-install.json").is_file();
    let registered = service.app(&id);
    if !source_install && !bundle_install {
        let app = registered.map_err(|error| error.to_string())?;
        if sabine_service::installed_by_store(&app) {
            service.remove_app(&id).map_err(|error| error.to_string())?;
            if purge {
                remove_data(&id, &directory)?;
            }
            report_uninstalled(&app.manifest.name, purge);
            return Ok(ExitCode::SUCCESS);
        }
        return Err(format!(
            "{} was not installed by sabine install; remove it through its package manager or OS uninstaller",
            app.manifest.name
        ));
    }
    let name = registered
        .map(|app| app.manifest.name)
        .or_else(|_| super::source::read_registered_app(&id).map(|app| app.name))
        .unwrap_or_else(|_| id.clone());
    let install = directory.join("install");
    remove_desktop(&id, &name, &directory)?;
    service
        .forget_app(&id, &install)
        .map_err(|error| error.to_string())?;
    if bundle_install && !purge {
        let payload = if cfg!(target_os = "macos") {
            install.join(format!("{id}.app"))
        } else {
            install.clone()
        };
        sabine_service::remove_app_payload(&payload, &id).map_err(|error| error.to_string())?;
    } else if purge {
        remove_path(&install)?;
    }
    for entry in [
        "launch.sh",
        "launch.cmd",
        "web",
        "icons",
        "source-install.toml",
        "bundle-install.json",
    ] {
        remove_path(&directory.join(entry))?;
    }
    if purge {
        remove_data(&id, &directory)?;
    } else {
        remove_empty_dir(&install)?;
        remove_empty_dir(&directory)?;
    }
    report_uninstalled(&name, purge);
    Ok(ExitCode::SUCCESS)
}

fn remove_data(id: &str, directory: &Path) -> Result<(), String> {
    remove_path(directory)?;
    let profile = sabine_service::browser_profile_path(id);
    remove_path(profile.parent().ok_or("browser profile has no parent")?)?;
    for shared in sabine_service::shared_browser_profile_paths(id) {
        remove_path(&shared)?;
    }
    Ok(())
}

fn report_uninstalled(name: &str, purge: bool) {
    println!(
        "Uninstalled {name}{}",
        if purge {
            " and its Sabine data"
        } else {
            "; app data was kept"
        }
    );
}

fn remove_desktop(id: &str, name: &str, directory: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let _ = id;
    #[cfg(not(windows))]
    let _ = name;
    #[cfg(any(target_os = "macos", windows))]
    let install = directory.join("install");
    #[cfg(target_os = "linux")]
    {
        sabine_service::forget_desktop_entries(id, directory).map_err(|error| error.to_string())?;
        let data = super::source::data_home()?;
        let applications = data.join("applications");
        remove_path(&applications.join(format!("{id}.desktop")))?;
        let mime_package = data.join("mime/packages").join(format!("{id}.xml"));
        if mime_package.is_file() {
            remove_path(&mime_package)?;
            super::desktop::refresh_mime_database(&data.join("mime"));
        }
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
        super::handlers::macos::remove(id, &install);
        let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
        remove_path(&home.join("Applications").join(format!("{id}.app")))?;
    }
    #[cfg(windows)]
    {
        super::handlers::windows::remove(&install)?;
        super::desktop::remove_windows_shortcut(name)?;
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

fn remove_empty_dir(path: &Path) -> Result<(), String> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(format!("could not remove {}: {error}", path.display())),
    }
}
