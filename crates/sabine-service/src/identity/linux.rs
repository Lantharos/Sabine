use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

use sabine_runtime::{configure_background_command, path_in_root};

use super::{ICON_NAME, ICON_PNGS, ICON_SVG};
use crate::app::{
    desktop::{data_home, desktop_exec},
    forget::remove_path,
};

const SYSTEM_SHARE: &str = "/usr/share";

/// Names Sabine and gives it its icon for this user, so task lists and system
/// monitors can show the shared runtime and service. A system that ships the
/// entry already covers every user.
pub(crate) fn install_for_user(daemon: &Path) -> io::Result<()> {
    if desktop_entry_path(Path::new(SYSTEM_SHARE)).is_file() {
        return Ok(());
    }
    install(&data_home()?, daemon)
}

/// Installs the entry and icons for every user into the system rooted at `root`.
pub(crate) fn install_for_system(root: Option<&Path>, daemon: &Path) -> io::Result<()> {
    install(&path_in_root(root, Path::new(SYSTEM_SHARE)), daemon)
}

pub(crate) fn remove_for_user() -> io::Result<()> {
    let share = data_home()?;
    remove_path(&desktop_entry_path(&share))?;
    for (path, _) in icon_paths(&share.join("icons/hicolor")) {
        remove_path(&path)?;
    }
    Ok(())
}

fn install(share: &Path, daemon: &Path) -> io::Result<()> {
    let hicolor = share.join("icons/hicolor");
    let mut icons_changed = false;
    for (path, contents) in icon_paths(&hicolor) {
        icons_changed |= write_if_changed(&path, contents)?;
    }
    write_if_changed(&desktop_entry_path(share), desktop_entry(daemon).as_bytes())?;
    if icons_changed && hicolor.join("icon-theme.cache").is_file() {
        let mut command = Command::new("gtk-update-icon-cache");
        configure_background_command(&mut command);
        let _ = command.args(["-q", "-t"]).arg(&hicolor).status();
    }
    Ok(())
}

fn icon_paths(hicolor: &Path) -> impl Iterator<Item = (PathBuf, &'static [u8])> {
    let scalable = (
        hicolor.join(format!("scalable/apps/{ICON_NAME}.svg")),
        ICON_SVG,
    );
    let sized = ICON_PNGS.iter().map(move |(size, png)| {
        (
            hicolor.join(format!("{size}x{size}/apps/{ICON_NAME}.png")),
            *png,
        )
    });
    std::iter::once(scalable).chain(sized)
}

fn desktop_entry_path(share: &Path) -> PathBuf {
    share
        .join("applications")
        .join(format!("{ICON_NAME}.desktop"))
}

fn desktop_entry(daemon: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Sabine\nComment=Sabine runtime and app service\nIcon={ICON_NAME}\nExec={}\nTerminal=false\nNoDisplay=true\n",
        desktop_exec(&daemon.to_string_lossy())
    )
}

fn write_if_changed(path: &Path, contents: &[u8]) -> io::Result<bool> {
    if fs::read(path).is_ok_and(|current| current == contents) {
        return Ok(false);
    }
    fs::create_dir_all(path.parent().expect("an icon or entry path has a folder"))?;
    fs::write(path, contents)?;
    Ok(true)
}
