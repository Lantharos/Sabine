use std::{io, path::PathBuf};

#[cfg(target_os = "linux")]
pub(crate) use appimage::running_appimage;

#[cfg(target_os = "linux")]
pub(crate) fn launch_executable() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    Ok(appimage::running_appimage_for(&executable).map_or(executable, |image| image.file))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn launch_executable() -> io::Result<PathBuf> {
    std::env::current_exe()
}

#[cfg(target_os = "linux")]
mod appimage {
    use std::{
        env,
        ffi::OsString,
        io,
        path::{Path, PathBuf},
    };

    pub(crate) struct AppImage {
        pub(crate) file: PathBuf,
        pub(crate) mount: PathBuf,
    }

    pub(crate) fn running_appimage() -> io::Result<Option<AppImage>> {
        Ok(running_appimage_for(&env::current_exe()?))
    }

    pub(super) fn running_appimage_for(executable: &Path) -> Option<AppImage> {
        appimage_containing(executable, env::var_os("APPIMAGE"), env::var_os("APPDIR"))
    }

    fn appimage_containing(
        executable: &Path,
        file: Option<OsString>,
        mount: Option<OsString>,
    ) -> Option<AppImage> {
        let file = PathBuf::from(file.filter(|file| !file.is_empty())?);
        let mount = PathBuf::from(mount.filter(|mount| !mount.is_empty())?);
        let mount = mount.canonicalize().unwrap_or(mount);
        executable
            .starts_with(&mount)
            .then_some(AppImage { file, mount })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn uses_the_appimage_only_when_running_from_its_mount() {
            let mount = || Some(OsString::from("/tmp/.mount_NotesAb12"));
            let image = || Some(OsString::from("/home/ana/Apps/Notes.AppImage"));
            let inside = Path::new("/tmp/.mount_NotesAb12/usr/lib/sabine/com.example.notes/notes");

            let running = appimage_containing(inside, image(), mount()).unwrap();
            assert_eq!(running.file, Path::new("/home/ana/Apps/Notes.AppImage"));

            let launched_by_another_appimage = Path::new("/usr/lib/sabine/com.example.notes/notes");
            assert!(appimage_containing(launched_by_another_appimage, image(), mount()).is_none());
            assert!(appimage_containing(inside, None, mount()).is_none());
        }
    }
}
