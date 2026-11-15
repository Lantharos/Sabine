//! The desktop's list of recently opened files: the jump list's recent items
//! on Windows, File > Open Recent and the Dock menu on macOS, and the
//! freedesktop `recently-used.xbel` that file managers and file choosers read
//! on Linux.

#[cfg(target_os = "linux")]
mod xbel;

use std::path::{Path, PathBuf};

/// Adds files to the desktop's recent list on the app's behalf, and takes
/// them off again.
#[cfg_attr(not(target_os = "linux"), derive(Default))]
pub struct RecentDocuments {
    #[cfg(target_os = "linux")]
    application: xbel::Application,
}

impl RecentDocuments {
    /// The list names the app by `name` and the `program` that opens its
    /// files.
    #[cfg(target_os = "linux")]
    pub fn new(name: &str, program: &Path) -> Self {
        Self {
            application: xbel::Application::new(name, program),
        }
    }

    pub fn add(&self, path: &Path) -> Result<(), String> {
        let file = existing_file(path)?;
        #[cfg(target_os = "linux")]
        return xbel::add(&file, &self.application).map_err(|error| error.to_string());
        #[cfg(not(target_os = "linux"))]
        platform::add(file)
    }

    /// Takes every file the app added off the list.
    pub fn clear(&self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        return xbel::clear(&self.application).map_err(|error| error.to_string());
        #[cfg(not(target_os = "linux"))]
        platform::clear()
    }
}

fn existing_file(path: &Path) -> Result<PathBuf, String> {
    dunce::canonicalize(path)
        .ok()
        .filter(|file| file.is_file())
        .ok_or_else(|| format!("{} is not a file", path.display()))
}

#[cfg(target_os = "windows")]
mod platform {
    // ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
    //
    // The shell keeps an app's recent files by its AppUserModelID, which
    // windows of the same executable share. It shows a file in the app's jump
    // list only when the file's type is registered to open with the app.
    // Shell calls want an apartment-threaded caller, so each runs on a thread
    // of its own.

    use std::path::PathBuf;

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        ApplicationDestinations, IApplicationDestinations, SHARD_PATHW, SHAddToRecentDocs,
    };
    use windows::core::HSTRING;

    pub(super) fn add(file: PathBuf) -> Result<(), String> {
        on_shell_thread(move || {
            let path = HSTRING::from(file.as_os_str());
            unsafe { SHAddToRecentDocs(SHARD_PATHW.0 as u32, Some(path.as_ptr().cast())) };
            Ok(())
        })
    }

    pub(super) fn clear() -> Result<(), String> {
        on_shell_thread(|| unsafe {
            CoCreateInstance::<_, IApplicationDestinations>(
                &ApplicationDestinations,
                None,
                CLSCTX_INPROC_SERVER,
            )?
            .RemoveAllDestinations()
        })
    }

    fn on_shell_thread(
        task: impl FnOnce() -> windows::core::Result<()> + Send + 'static,
    ) -> Result<(), String> {
        std::thread::spawn(move || unsafe {
            let initialized =
                CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
            let result = task();
            if initialized {
                CoUninitialize();
            }
            result
        })
        .join()
        .map_err(|_| "the shell call panicked".to_string())?
        .map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::PathBuf;

    use dispatch2::DispatchQueue;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSDocumentController;
    use objc2_foundation::{NSString, NSURL};

    pub(super) fn add(file: PathBuf) -> Result<(), String> {
        let path = file.to_string_lossy().into_owned();
        DispatchQueue::main().exec_async(move || {
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path));
            NSDocumentController::sharedDocumentController(main_thread())
                .noteNewRecentDocumentURL(&url);
        });
        Ok(())
    }

    pub(super) fn clear() -> Result<(), String> {
        DispatchQueue::main().exec_async(|| {
            unsafe {
                NSDocumentController::sharedDocumentController(main_thread())
                    .clearRecentDocuments(None)
            };
        });
        Ok(())
    }

    fn main_thread() -> MainThreadMarker {
        // SAFETY: tasks on the main dispatch queue run on the main thread.
        unsafe { MainThreadMarker::new_unchecked() }
    }
}
