use std::{
    path::Path,
    process::{Command, Stdio},
};

/// Rebuilds a theme's icon cache, which hides icons added after it was made.
/// `-t` is for a user's own theme folder, which has no index of its own.
pub fn refresh_icon_cache(theme: &Path) {
    if theme.is_dir() {
        refresh("gtk-update-icon-cache", &["-q", "-f", "-t"], theme);
    }
}

pub fn refresh_desktop_database(applications: &Path) {
    refresh("update-desktop-database", &[], applications);
}

pub fn refresh_mime_database(mime: &Path) {
    refresh("update-mime-database", &[], mime);
}

fn refresh(tool: &str, options: &[&str], directory: &Path) {
    if sabine_runtime::find_program(tool).is_none() {
        return;
    }
    let _ = Command::new(tool)
        .args(options)
        .arg(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status();
}
