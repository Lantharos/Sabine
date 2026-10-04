use sabine_service::SabineService;
#[cfg(any(windows, target_os = "macos"))]
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
    let directory = service.root().join("apps").join(&id);
    let source_install = directory.join("source-install.toml").is_file();
    let bundle_install = directory.join("bundle-install.json").is_file();
    let registered = service.app(&id);
    if !source_install && !bundle_install {
        let app = registered.map_err(|error| error.to_string())?;
        return Err(format!(
            "{} was not installed by sabine install; remove it through its package manager or OS uninstaller",
            app.manifest.name
        ));
    }
    let name = registered.map_or_else(|_| id.clone(), |app| app.manifest.name);
    let install = directory.join("install");
    remove_desktop(&id, &install)?;
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
        remove_path(&directory)?;
        let profile = sabine_service::browser_profile_path(&id);
        remove_path(profile.parent().ok_or("browser profile has no parent")?)?;
    } else {
        remove_empty_dir(&install)?;
        remove_empty_dir(&directory)?;
    }
    println!(
        "Uninstalled {name}{}",
        if purge {
            " and its Sabine data"
        } else {
            "; app data was kept"
        }
    );
    Ok(ExitCode::SUCCESS)
}

fn remove_desktop(id: &str, install: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        super::handlers::linux::remove(id, install)?;
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
        super::handlers::macos::remove(id, install);
        let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?);
        remove_path(&home.join("Applications").join(format!("{id}.app")))?;
    }
    #[cfg(windows)]
    {
        super::handlers::windows::remove(install)?;
        let roaming = PathBuf::from(std::env::var_os("APPDATA").ok_or("APPDATA is not set")?);
        remove_path(
            &roaming
                .join("Microsoft/Windows/Start Menu/Programs")
                .join(format!("{id}.lnk")),
        )?;
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
