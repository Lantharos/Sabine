//! Opening web and mail links in the person's default apps, and folders in
//! their file manager.

use std::path::Path;

/// Web and mail links; other schemes could start arbitrary programs.
pub fn is_openable_link(url: &str) -> bool {
    let scheme = url
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase());
    matches!(scheme.as_deref(), Some("http" | "https" | "mailto"))
        && !url.chars().any(char::is_control)
}

pub async fn open_link(url: &str, app_id: Option<&str>) -> Result<(), String> {
    if !is_openable_link(url) {
        return Err("Only web and mail links can be opened".to_string());
    }
    platform::open_link(url, app_id).await
}

pub async fn show_folder(path: &Path, app_id: Option<&str>) -> Result<(), String> {
    let folder = path
        .canonicalize()
        .ok()
        .filter(|folder| folder.is_dir())
        .ok_or_else(|| format!("{} is not a folder", path.display()))?;
    platform::show_folder(&folder, app_id).await
}

#[cfg(target_os = "linux")]
mod platform {
    use std::fs::File;
    use std::os::fd::AsFd;
    use std::path::Path;

    use zbus::{Connection, Fd, OwnedObjectPath, Variant};

    use crate::portal::{self, Options};

    const OPEN_URI: &str = "org.freedesktop.portal.OpenURI";

    async fn connect(app_id: Option<&str>) -> zbus::Result<Connection> {
        let connection = Connection::session().await?;
        if let Some(app_id) = app_id {
            portal::register_host_app(&connection, app_id).await?;
        }
        Ok(connection)
    }

    fn options() -> Options<'static> {
        Options::from([("handle_token", Variant::from(portal::token()))])
    }

    pub(super) async fn open_link(url: &str, app_id: Option<&str>) -> Result<(), String> {
        let connection = connect(app_id).await.map_err(unavailable)?;
        portal::proxy(&connection, OPEN_URI)
            .await
            .map_err(unavailable)?
            .call::<_, _, OwnedObjectPath>("OpenURI", &("", url, options()))
            .await
            .map(drop)
            .map_err(unavailable)
    }

    pub(super) async fn show_folder(path: &Path, app_id: Option<&str>) -> Result<(), String> {
        let folder = File::open(path).map_err(|error| error.to_string())?;
        let connection = connect(app_id).await.map_err(unavailable)?;
        portal::proxy(&connection, OPEN_URI)
            .await
            .map_err(unavailable)?
            .call::<_, _, OwnedObjectPath>("OpenFile", &("", Fd::from(folder.as_fd()), options()))
            .await
            .map(drop)
            .map_err(unavailable)
    }

    fn unavailable(error: zbus::Error) -> String {
        format!("The desktop couldn't open it: {error}")
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::path::Path;

    use windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, w};

    pub(super) async fn open_link(url: &str, _app_id: Option<&str>) -> Result<(), String> {
        shell_open(HSTRING::from(url)).await
    }

    pub(super) async fn show_folder(path: &Path, _app_id: Option<&str>) -> Result<(), String> {
        shell_open(HSTRING::from(path.as_os_str())).await
    }

    /// Runs the shell's open verb on a thread of its own, since shell
    /// extensions it hands the target to expect an apartment-threaded caller.
    async fn shell_open(target: HSTRING) -> Result<(), String> {
        tokio::task::spawn_blocking(move || unsafe {
            let initialized =
                CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
            let instance = ShellExecuteW(None, w!("open"), &target, None, None, SW_SHOWNORMAL);
            if initialized {
                CoUninitialize();
            }
            if instance.0 as isize > 32 {
                Ok(())
            } else {
                Err(format!("Windows couldn't open {target}"))
            }
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::OsStr;
    use std::path::Path;
    use std::process::Command;

    pub(super) async fn open_link(url: &str, _app_id: Option<&str>) -> Result<(), String> {
        open(OsStr::new(url)).await
    }

    pub(super) async fn show_folder(path: &Path, _app_id: Option<&str>) -> Result<(), String> {
        open(path.as_os_str()).await
    }

    async fn open(target: &OsStr) -> Result<(), String> {
        let target = target.to_owned();
        tokio::task::spawn_blocking(move || {
            let status = Command::new("/usr/bin/open")
                .arg(&target)
                .status()
                .map_err(|error| error.to_string())?;
            status
                .success()
                .then_some(())
                .ok_or_else(|| format!("macOS couldn't open {}", target.to_string_lossy()))
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::is_openable_link;

    #[test]
    fn opens_only_web_and_mail_links() {
        assert!(is_openable_link("https://example.com/a?b"));
        assert!(is_openable_link("HTTP://example.com"));
        assert!(is_openable_link("mailto:someone@example.com"));
        assert!(!is_openable_link("file:///etc/passwd"));
        assert!(!is_openable_link("javascript:alert(1)"));
        assert!(!is_openable_link("ms-settings:"));
        assert!(!is_openable_link("https://example.com/\nmore"));
    }
}
