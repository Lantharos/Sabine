use super::stage::StagedBundle;
use crate::{
    install::{source::InstallOptions, system::SystemInstall},
    project::BundleApp,
};
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
    let (app, staged) = super::stage_portable(&options, env_files, temporary.path(), true)?;
    let placed = place(&app, &staged, root)?;
    SystemInstall {
        name: app.name.clone(),
        paths: placed.paths,
    }
    .write(&app.id, root)?;
    println!(
        "Installed {} {} for every user at {}",
        app.name,
        app.version,
        placed.executable.display()
    );
    Ok(ExitCode::SUCCESS)
}

/// The app's program, and what was placed for it, as the machine sees it.
struct Placed {
    executable: PathBuf,
    paths: Vec<PathBuf>,
}

#[cfg(target_os = "linux")]
fn place(app: &BundleApp, staged: &StagedBundle, root: Option<&Path>) -> Result<Placed, String> {
    let destination = sabine_runtime::path_in_root(root, Path::new("/"));
    let program = Path::new("/usr/lib/sabine").join(&app.id);
    remove_directory(&sabine_runtime::path_in_root(root, &program))?;
    let mut files = Vec::new();
    merge_tree(&staged.app_dir, &destination, Path::new("/"), &mut files)
        .map_err(|error| error.to_string())?;
    if root.is_none() {
        sabine_service::refresh_desktop_database(Path::new("/usr/share/applications"));
        sabine_service::refresh_mime_database(Path::new("/usr/share/mime"));
        sabine_service::refresh_icon_cache(Path::new("/usr/share/icons/hicolor"));
    }
    let mut paths = vec![program.clone()];
    paths.extend(files.into_iter().filter(|file| !file.starts_with(&program)));
    Ok(Placed {
        executable: destination.join("usr/bin").join(&staged.executable),
        paths,
    })
}

#[cfg(target_os = "macos")]
fn place(_: &BundleApp, staged: &StagedBundle, root: Option<&Path>) -> Result<Placed, String> {
    let bundle = staged
        .app_dir
        .file_name()
        .ok_or("the staged app has no name")?;
    let installed = Path::new("/Applications").join(bundle);
    let destination = sabine_runtime::path_in_root(root, &installed);
    remove_directory(&destination)?;
    merge_tree(&staged.app_dir, &destination, &installed, &mut Vec::new())
        .map_err(|error| error.to_string())?;
    Ok(Placed {
        executable: destination.join("Contents/MacOS").join(&staged.executable),
        paths: vec![installed],
    })
}

#[cfg(windows)]
fn place(app: &BundleApp, staged: &StagedBundle, root: Option<&Path>) -> Result<Placed, String> {
    use crate::install::shortcut::{Shortcut, StartMenu};
    let installed = sabine_runtime::program_files().join(super::metadata::sanitize_path(&app.name));
    let destination = sabine_runtime::path_in_root(root, &installed);
    remove_directory(&destination)?;
    merge_tree(&staged.app_dir, &destination, &installed, &mut Vec::new())
        .map_err(|error| error.to_string())?;
    let executable = destination.join(&staged.executable);
    let mut paths = vec![installed];
    if root.is_none() && app.listing.listed {
        let icon = destination.join("resources/windows-app.ico");
        paths.push(
            Shortcut {
                name: &app.name,
                target: &executable,
                arguments: "",
                description: app.listing.generic_name.as_deref(),
                icon: icon
                    .is_file()
                    .then(|| icon.to_string_lossy().into_owned())
                    .as_deref(),
            }
            .install(StartMenu::AllUsers)?,
        );
    }
    Ok(Placed { executable, paths })
}

fn remove_directory(path: &Path) -> Result<(), String> {
    sabine_runtime::remove_directory_if_present(path)
        .map_err(|error| format!("could not replace {}: {error}", path.display()))
}

/// Copies `source` into `destination`, replacing files that already exist and
/// keeping symbolic links as links. Each copy is noted in `files` as the
/// path it has below `installed`.
fn merge_tree(
    source: &Path,
    destination: &Path,
    installed: &Path,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let kind = entry.file_type()?;
        let placed = installed.join(entry.file_name());
        if kind.is_dir() {
            merge_tree(&from, &to, &placed, files)?;
            continue;
        }
        files.push(placed);
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
