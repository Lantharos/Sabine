//! An app has one login item, named after its app id. The app writes it when it
//! runs, and every uninstaller removes it through [`remove_app_autostart`].

use std::io;
#[cfg(unix)]
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
pub fn app_autostart_path(id: &str) -> io::Result<PathBuf> {
    Ok(super::config_home()?
        .join("autostart")
        .join(format!("{id}.desktop")))
}

#[cfg(target_os = "macos")]
pub fn app_autostart_label(id: &str) -> String {
    format!("dev.sabine.{id}")
}

#[cfg(target_os = "macos")]
pub fn app_autostart_path(id: &str) -> io::Result<PathBuf> {
    Ok(super::home_dir()?
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", app_autostart_label(id))))
}

/// The per-user `Run` key; each app's value is named after its app id.
#[cfg(windows)]
pub const APP_AUTOSTART_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

pub fn remove_app_autostart(id: &str) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    remove_file(&app_autostart_path(id)?)?;
    #[cfg(target_os = "macos")]
    {
        let service = format!(
            "gui/{}/{}",
            unsafe { libc::getuid() },
            app_autostart_label(id)
        );
        let _ = std::process::Command::new("launchctl")
            .args(["bootout", &service])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        remove_file(&app_autostart_path(id)?)?;
    }
    #[cfg(windows)]
    crate::windows_registry::delete_current_user_value(APP_AUTOSTART_KEY, id)?;
    Ok(())
}

#[cfg(unix)]
fn remove_file(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}
