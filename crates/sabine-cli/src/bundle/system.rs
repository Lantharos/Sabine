use super::{BundleFormat, BundleOptions, metadata::sanitize_path, prepare_bundle};
use crate::install::source::InstallOptions;
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
};

/// Installs a production bundle for every user of the machine, or stages it
/// under `root`. Apps find the machine's Sabine service and runtime, so their
/// first launch needs no setup; each user's app data still lives in their own
/// folders. The operating system owns updates of system-wide apps.
pub fn install_system_bundle(
    options: InstallOptions,
    env_files: Vec<PathBuf>,
    root: Option<&Path>,
) -> Result<ExitCode, String> {
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let target = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let (app, format, staged) = prepare_bundle(BundleOptions {
        source: options.source,
        target: target.into(),
        out: temporary.path().to_path_buf(),
        release: true,
        no_build: false,
        binary: None,
        no_web_build: false,
        web_build: None,
        web_root: None,
        web_dist: None,
        id: options.id,
        name: options.name,
        version: None,
        json: false,
        offline: false,
        system_wide: true,
        env_files,
    })?;
    let executable = match format {
        BundleFormat::Linux => {
            let destination = sabine_runtime::path_in_root(root, Path::new("/"));
            remove_directory(&destination.join("usr/lib/sabine").join(&app.id))?;
            merge_tree(&staged.app_dir, &destination).map_err(|error| error.to_string())?;
            if root.is_none() {
                refresh_linux_databases();
            }
            destination.join("usr/bin").join(&staged.executable)
        }
        BundleFormat::Macos => {
            let bundle = staged
                .app_dir
                .file_name()
                .ok_or("the staged app has no name")?;
            let destination =
                sabine_runtime::path_in_root(root, Path::new("/Applications")).join(bundle);
            remove_directory(&destination)?;
            merge_tree(&staged.app_dir, &destination).map_err(|error| error.to_string())?;
            destination.join("Contents/MacOS").join(&staged.executable)
        }
        _ => {
            let destination =
                sabine_runtime::path_in_root(root, &program_files()).join(sanitize_path(&app.name));
            remove_directory(&destination)?;
            merge_tree(&staged.app_dir, &destination).map_err(|error| error.to_string())?;
            let executable = destination.join(&staged.executable);
            #[cfg(windows)]
            if root.is_none() && app.listing.listed {
                let icon = destination.join("resources/windows-app.ico");
                crate::install::desktop::install_windows_shortcut(
                    &app.id,
                    app.listing.generic_name.as_deref(),
                    &executable,
                    icon.is_file()
                        .then(|| icon.to_string_lossy().into_owned())
                        .as_deref(),
                    true,
                )?;
            }
            executable
        }
    };
    println!(
        "Installed {} {} for every user at {}",
        app.name,
        app.version,
        executable.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn program_files() -> PathBuf {
    std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
}

fn remove_directory(path: &Path) -> Result<(), String> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(format!("could not replace {}: {error}", path.display()))
        }
        _ => Ok(()),
    }
}

/// Copies `source` into `destination`, replacing files that already exist and
/// keeping symbolic links as links.
fn merge_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            merge_tree(&from, &to)?;
            continue;
        }
        if to.symlink_metadata().is_ok() {
            fs::remove_file(&to)?;
        }
        if kind.is_symlink() {
            let target = fs::read_link(&from)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &to)?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(target, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn refresh_linux_databases() {
    #[cfg(target_os = "linux")]
    {
        use crate::install::desktop;
        desktop::refresh_database(Path::new("/usr/share/applications"));
        desktop::refresh_mime_database(Path::new("/usr/share/mime"));
        desktop::refresh_icon_cache(Path::new("/usr/share/icons/hicolor"));
    }
}
