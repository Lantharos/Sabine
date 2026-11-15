use std::path::{Path, PathBuf};

use windows::{
    Win32::{
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize, IPersistFile,
        },
        UI::Shell::{
            FOLDERID_CommonPrograms, FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT,
            SHGetKnownFolderPath, ShellLink,
        },
    },
    core::{HSTRING, Interface},
};

#[derive(Clone, Copy)]
pub(crate) enum StartMenu {
    User,
    AllUsers,
}

impl StartMenu {
    fn folder(self) -> Result<PathBuf, String> {
        let id = match self {
            Self::User => FOLDERID_Programs,
            Self::AllUsers => FOLDERID_CommonPrograms,
        };
        unsafe {
            let path = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None)
                .map_err(|error| format!("could not find the Start menu: {error}"))?;
            let folder = path.to_string().map(PathBuf::from);
            CoTaskMemFree(Some(path.0.cast_const().cast()));
            folder.map_err(|error| error.to_string())
        }
    }

    /// The Start menu lists a shortcut by its file name, so it is the app's name.
    pub(crate) fn shortcut(self, app_name: &str) -> Result<PathBuf, String> {
        let name = app_name
            .chars()
            .map(|character| {
                if character.is_control() || r#"\/:*?"<>|"#.contains(character) {
                    '-'
                } else {
                    character
                }
            })
            .collect::<String>();
        Ok(self
            .folder()?
            .join(format!("{}.lnk", name.trim_end_matches(['.', ' ']))))
    }
}

pub(crate) struct Shortcut<'a> {
    pub name: &'a str,
    pub target: &'a Path,
    pub arguments: &'a str,
    pub description: Option<&'a str>,
    pub icon: Option<&'a str>,
}

impl Shortcut<'_> {
    pub(crate) fn install(&self, menu: StartMenu) -> Result<PathBuf, String> {
        let path = menu.shortcut(self.name)?;
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let saved = self.save(&path);
        if initialized {
            unsafe { CoUninitialize() };
        }
        saved
            .map(|()| path)
            .map_err(|error| format!("failed to create the Start menu shortcut: {error}"))
    }

    fn save(&self, path: &Path) -> windows::core::Result<()> {
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&HSTRING::from(self.target))?;
            link.SetArguments(&HSTRING::from(self.arguments))?;
            if let Some(description) = self.description {
                link.SetDescription(&HSTRING::from(description))?;
            }
            if let Some(icon) = self.icon {
                link.SetIconLocation(&HSTRING::from(icon), 0)?;
            }
            link.cast::<IPersistFile>()?
                .Save(&HSTRING::from(path), true)
        }
    }
}

pub(crate) fn remove(app_name: &str, menu: StartMenu) -> Result<(), String> {
    sabine_runtime::remove_file_if_present(&menu.shortcut(app_name)?)
        .map_err(|error| error.to_string())
}
