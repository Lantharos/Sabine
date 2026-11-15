// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The jump list belongs to the AppUserModelID the app's windows share, which
// unpackaged windows of one executable get from its path. Its tasks are
// shell links to the executable with each action's arguments, named through
// the link's `PKEY_Title`. An app without actions removes the tasks an older
// version of it listed.

use sabine_service::manifest::AppAction;
use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
    CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, SHStrDupW,
    ShellLink,
};
use windows::core::{HSTRING, Interface, PCWSTR};

/// Lists the app's actions as the tasks of its jump list.
pub(in crate::desktop) fn register_app_actions(actions: &[AppAction]) -> Result<(), String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    let program = HSTRING::from(program.as_os_str());
    unsafe {
        let initialized =
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        let listed = list(&program, actions);
        if initialized {
            CoUninitialize();
        }
        listed.map_err(|error| error.to_string())
    }
}

unsafe fn list(program: &HSTRING, actions: &[AppAction]) -> windows::core::Result<()> {
    unsafe {
        let list: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        if actions.is_empty() {
            return list.DeleteList(PCWSTR::null());
        }
        let mut slots = 0;
        let _removed: IObjectArray = list.BeginList(&mut slots)?;
        let tasks: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
        for action in actions {
            tasks.AddObject(&task(program, action)?)?;
        }
        list.AddUserTasks(&tasks.cast::<IObjectArray>()?)?;
        list.CommitList()
    }
}

unsafe fn task(program: &HSTRING, action: &AppAction) -> windows::core::Result<IShellLinkW> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(program)?;
        link.SetArguments(&HSTRING::from(command_line(&action.arguments)))?;
        link.SetIconLocation(program, 0)?;
        let properties = link.cast::<IPropertyStore>()?;
        properties.SetValue(&PKEY_Title, &text(&action.name)?)?;
        properties.Commit()?;
        Ok(link)
    }
}

/// A `VT_LPWSTR` value, which the shell reads titles from.
unsafe fn text(value: &str) -> windows::core::Result<PROPVARIANT> {
    let mut variant = PROPVARIANT::default();
    unsafe {
        let value = SHStrDupW(&HSTRING::from(value))?;
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_LPWSTR;
        inner.Anonymous.pwszVal = value;
    }
    Ok(variant)
}

/// Arguments joined the way `CommandLineToArgvW` splits them again.
fn command_line(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| quote(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quote(argument: &str) -> String {
    if !argument.is_empty() && !argument.contains([' ', '\t', '"']) {
        return argument.to_string();
    }
    let mut quoted = String::from('"');
    let mut backslashes = 0;
    for character in argument.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(character);
                backslashes = 0;
            }
        }
    }
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::command_line;

    #[test]
    fn arguments_survive_the_command_line() {
        let arguments = ["--new-window", "two words", r#"say "hi""#, r"C:\dir\"].map(String::from);
        assert_eq!(
            command_line(&arguments),
            r#"--new-window "two words" "say \"hi\"" C:\dir\"#
        );
    }
}
