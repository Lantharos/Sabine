//! Opening web and mail links in the person's default apps, and folders in
//! their file manager.

use std::path::Path;

#[cfg(target_os = "linux")]
pub use crate::linux::ParentWindow;

/// Where a request to open something comes from. On Linux the desktop
/// portal names the app by its id, keeps any dialog it shows over the parent
/// window, which `ParentWindow::identifier` names, and hands the activation
/// token's focus to the app that opens.
#[derive(Default)]
pub struct Requester {
    pub app_id: Option<String>,
    #[cfg(target_os = "linux")]
    pub parent: String,
    #[cfg(target_os = "linux")]
    pub activation_token: Option<String>,
}

pub async fn open_link(url: &str, requester: Requester) -> Result<(), String> {
    if !is_openable_link(url) {
        return Err("Only web and mail links can be opened".to_string());
    }
    platform::open_link(url, requester).await
}

pub async fn show_folder(path: &Path, requester: Requester) -> Result<(), String> {
    let folder = dunce::canonicalize(path)
        .ok()
        .filter(|folder| folder.is_dir())
        .ok_or_else(|| format!("{} is not a folder", path.display()))?;
    platform::show_folder(&folder, requester).await
}

/// Web and mail links; other schemes could start arbitrary programs.
fn is_openable_link(url: &str) -> bool {
    let scheme = url
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase());
    matches!(scheme.as_deref(), Some("http" | "https" | "mailto"))
        && !url.chars().any(char::is_control)
}

#[cfg(target_os = "linux")]
mod platform {
    use std::fs::File;
    use std::os::fd::AsFd;
    use std::path::Path;

    use zbus::{Connection, Fd, OwnedObjectPath, Proxy, Variant};

    use super::Requester;
    use crate::portal::{self, Options, Response};

    const OPEN_URI: &str = "org.freedesktop.portal.OpenURI";

    pub(super) async fn open_link(url: &str, requester: Requester) -> Result<(), String> {
        let open_uri = &OpenUri::connect(&requester).await.map_err(unavailable)?;
        let response = portal::respond(&open_uri.connection, |token| async move {
            let body = (&open_uri.parent, url, open_uri.options(token));
            open_uri
                .proxy
                .call::<_, _, OwnedObjectPath>("OpenURI", &body)
                .await
        })
        .await;
        opened(response)
    }

    pub(super) async fn show_folder(path: &Path, requester: Requester) -> Result<(), String> {
        let folder = &File::open(path).map_err(|error| error.to_string())?;
        let open_uri = &OpenUri::connect(&requester).await.map_err(unavailable)?;
        let response = portal::respond(&open_uri.connection, |token| async move {
            let body = (
                &open_uri.parent,
                Fd::from(folder.as_fd()),
                open_uri.options(token),
            );
            open_uri
                .proxy
                .call::<_, _, OwnedObjectPath>("OpenFile", &body)
                .await
        })
        .await;
        opened(response)
    }

    struct OpenUri<'a> {
        connection: Connection,
        proxy: Proxy<'static>,
        parent: String,
        activation_token: Option<&'a str>,
    }

    impl<'a> OpenUri<'a> {
        async fn connect(requester: &'a Requester) -> zbus::Result<Self> {
            let connection = Connection::session().await?;
            if let Some(app_id) = &requester.app_id {
                portal::register_host_app(&connection, app_id).await?;
            }
            Ok(Self {
                proxy: portal::proxy(&connection, OPEN_URI).await?,
                connection,
                parent: requester.parent.clone(),
                activation_token: requester.activation_token.as_deref(),
            })
        }

        fn options(&self, handle_token: String) -> Options<'a> {
            let mut options = Options::from([("handle_token", Variant::from(handle_token))]);
            if let Some(token) = self.activation_token {
                options.insert("activation_token", Variant::from(token));
            }
            options
        }
    }

    /// The desktop answers once it opened the target, or once the person
    /// closed the app chooser it showed.
    fn opened(response: zbus::Result<Response>) -> Result<(), String> {
        match response.map_err(unavailable)? {
            Response::Answered(_) | Response::Cancelled => Ok(()),
            Response::Ended => Err("The desktop couldn't open it".to_string()),
        }
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

    use super::Requester;

    pub(super) async fn open_link(url: &str, _requester: Requester) -> Result<(), String> {
        shell_open(HSTRING::from(url)).await
    }

    pub(super) async fn show_folder(path: &Path, _requester: Requester) -> Result<(), String> {
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
    use std::path::Path;

    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSString, NSURL};

    use super::Requester;

    pub(super) async fn open_link(url: &str, _requester: Requester) -> Result<(), String> {
        let opened = NSURL::URLWithString(&NSString::from_str(url))
            .is_some_and(|link| NSWorkspace::sharedWorkspace().openURL(&link));
        opened
            .then_some(())
            .ok_or_else(|| format!("macOS couldn't open {url}"))
    }

    /// Opens a Finder window on the folder itself, so a package such as an
    /// app bundle shows its contents instead of launching.
    pub(super) async fn show_folder(path: &Path, _requester: Requester) -> Result<(), String> {
        let folder = NSString::from_str(&path.to_string_lossy());
        NSWorkspace::sharedWorkspace()
            .selectFile_inFileViewerRootedAtPath(None, &folder)
            .then_some(())
            .ok_or_else(|| format!("Finder couldn't show {}", path.display()))
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
