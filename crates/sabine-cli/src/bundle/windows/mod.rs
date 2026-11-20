mod artwork;
#[cfg(windows)]
mod certificate;
mod msi;
mod msi_actions;
pub(super) mod package;
mod pe;
pub(super) mod signing;

pub(super) use pe::WindowsArchitecture;

// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// NSIS strings have their own escaping rules, and cancellation must reach the
// running setup helper. Keep the per-user registry paths and payload inventory
// aligned with uninstall; this is not a shell script with a wizard attached.

use crate::{desktop::types::prog_id, project::BundleApp};
use signing::WindowsSigning;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn nsis_script(
    app: &BundleApp,
    source: &Path,
    executable: &str,
    output: &str,
    icon: Option<&str>,
    welcome_bitmap: Option<&Path>,
    signing: Option<&WindowsSigning>,
) -> Result<String, String> {
    let source = dunce::canonicalize(source).map_err(|error| error.to_string())?;
    let name = escape(&app.name);
    let id = escape(&app.id);
    let executable = escape(executable);
    let links = runtime_links(&source)?;
    let uninstall_files = uninstall_commands(&source, &source, &links)?;
    let setup = if app.standalone {
        Setup::standalone(&name)
    } else {
        let mut later = links;
        later.push("Uninstall.exe".into());
        sabine_service::write_install_inventory(&source, &app.id, &later)
            .map_err(|error| error.to_string())?;
        Setup::shared(&name)
    };
    let Setup {
        welcome,
        payload,
        clear_previous,
        install_to,
        message,
    } = setup;
    let icon = icon
        .map(|path| {
            format!(
                "!define MUI_ICON \"{}\"\n!define MUI_UNICON \"{}\"",
                escape(&dunce::simplified(Path::new(path)).to_string_lossy()),
                escape(&dunce::simplified(Path::new(path)).to_string_lossy())
            )
        })
        .unwrap_or_default();
    let welcome_bitmap = welcome_bitmap
        .map(|path| {
            format!(
                "!define MUI_WELCOMEFINISHPAGE_BITMAP \"{}\"\n",
                escape(&dunce::simplified(path).to_string_lossy())
            )
        })
        .unwrap_or_default();
    let sign_uninstaller = signing
        .map(WindowsSigning::nsis_uninstaller)
        .unwrap_or_default();
    let (register_handlers, unregister_handlers) = handler_commands(app, &name, &executable);
    let (finish_run, start_menu, start_menu_removal) = if app.listing.listed {
        (
            format!(
                r#"!define MUI_FINISHPAGE_RUN "$INSTDIR\{executable}"
!define MUI_FINISHPAGE_RUN_NOTCHECKED
!define MUI_FINISHPAGE_SHOWREADME ""
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Add a shortcut to the desktop"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION AddDesktopShortcut
Function AddDesktopShortcut
  SetShellVarContext current
  CreateShortcut "$DESKTOP\{name}.lnk" "$INSTDIR\{executable}"
FunctionEnd
"#
            ),
            format!(
                r#"  CreateDirectory "$SMPROGRAMS\{name}"
  CreateShortcut "$SMPROGRAMS\{name}\{name}.lnk" "$INSTDIR\{executable}"
  CreateShortcut "$SMPROGRAMS\{name}\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
"#
            ),
            format!(
                r#"  Delete "$SMPROGRAMS\{name}\{name}.lnk"
  Delete "$SMPROGRAMS\{name}\Uninstall.lnk"
  RMDir "$SMPROGRAMS\{name}"
  Delete "$DESKTOP\{name}.lnk"
"#
            ),
        )
    } else {
        Default::default()
    };
    Ok(format!(
        r##"Unicode true
!include "MUI2.nsh"
!include "WinMessages.nsh"
Name "{name}"
OutFile "{output}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\{id}"
InstallDirRegKey HKCU "Software\{id}" "InstallDir"
SetCompressor /SOLID lzma
{sign_uninstaller}ShowInstDetails show
ShowUninstDetails show
Var SetupRunning
Var SetupCancelled
Var CancelHandle
Var CancelButton
Var ProgressBar
Var ProgressStart
{icon}
{welcome_bitmap}!define MUI_CUSTOMFUNCTION_ABORT CancelSetup
!define MUI_WELCOMEPAGE_TEXT "{welcome}"
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
{finish_run}!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function CancelSetup
  StrCmp $SetupRunning "1" 0 cancel_normal
  StrCpy $SetupCancelled "1"
  FileOpen $CancelHandle "$PLUGINSDIR\cancel" w
  FileClose $CancelHandle
  GetDlgItem $CancelButton $HWNDPARENT 2
  EnableWindow $CancelButton 0
  DetailPrint "Cancelling setup..."
  Abort
cancel_normal:
FunctionEnd

Section "Install"
  SetShellVarContext current
  InitPluginsDir
  ClearErrors
{clear_previous}  SetOutPath "{payload}"
  File /r "{source}"
  WriteUninstaller "{payload}\Uninstall.exe"
  IfErrors setup_failed
  StrCpy $ProgressBar 0
  StrCpy $ProgressStart 0
  IfSilent progress_found
  FindWindow $ProgressBar "#32770" "" $HWNDPARENT
  GetDlgItem $ProgressBar $ProgressBar 1004
  SendMessage $ProgressBar ${{PBM_GETPOS}} 0 0 $ProgressStart
progress_found:
setup_retry:
  SendMessage $ProgressBar ${{PBM_SETPOS}} $ProgressStart 0
  Delete "$PLUGINSDIR\cancel"
  StrCpy $SetupCancelled "0"
  StrCpy $SetupRunning "1"
  GetDlgItem $CancelButton $HWNDPARENT 2
  EnableWindow $CancelButton 1
  DetailPrint "{message}"
  nsExec::ExecToLog '"{payload}\{executable}" --sabine-install{install_to} --sabine-install-cancel "$PLUGINSDIR\cancel" --sabine-install-progress-bar $ProgressBar'
  Pop $0
  StrCpy $SetupRunning "0"
  GetDlgItem $CancelButton $HWNDPARENT 2
  EnableWindow $CancelButton 0
  StrCmp $0 "0" setup_ready
  StrCmp $SetupCancelled "1" setup_cancelled
  IfSilent setup_failed
  MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Setup could not finish. The details above explain the failure. Close the application if it is running, check your connection and retry." IDRETRY setup_retry
setup_failed:
  SetErrorLevel 1
  Abort
setup_cancelled:
  SetErrorLevel 1602
  Abort "Installation cancelled."
setup_ready:
  WriteRegStr HKCU "Software\{id}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\{id}" "DisplayName" "{name}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\{id}" "DisplayVersion" "{version}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\{id}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\{id}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\{id}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\{id}" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\{id}" "NoRepair" 1
{register_handlers}{start_menu}SectionEnd

Section "Uninstall"
  SetShellVarContext current
  IfFileExists "$INSTDIR\{executable}" 0 unregister_done
unregister_retry:
  nsExec::ExecToLog '"$INSTDIR\{executable}" --sabine-uninstall'
  Pop $0
  StrCmp $0 "0" unregister_done
  IfSilent unregister_failed
  MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "The application could not be unregistered. Close the application and retry." IDRETRY unregister_retry
unregister_failed:
  SetErrorLevel 1
  Abort
unregister_done:
{uninstall_files}
  Delete "$INSTDIR\.sabine-install.json"
{start_menu_removal}{unregister_handlers}  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\{id}"
  DeleteRegKey HKCU "Software\{id}"
  IfSilent data_kept
  IfFileExists "$APPDATA\{id}\*.*" 0 data_kept
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Also delete the files {name} keeps for you?$\r$\n$\r$\n$APPDATA\{id}$\r$\n$\r$\nChoose No to keep them for a later install." IDNO data_kept
  RMDir /r "$APPDATA\{id}"
  RMDir /r "$LOCALAPPDATA\{id}"
data_kept:
SectionEnd
"##,
        output = escape(&dunce::simplified(Path::new(output)).to_string_lossy()),
        source = escape(&source.join("*").display().to_string()),
        version = escape(&app.version),
    ))
}

/// How setup puts the app in place: through the app itself into the
/// computer's Sabine, or, for a standalone app, by copying it there.
struct Setup {
    welcome: String,
    payload: &'static str,
    clear_previous: &'static str,
    install_to: &'static str,
    message: String,
}

impl Setup {
    fn shared(name: &str) -> Self {
        Self {
            welcome: format!(
                r"Setup installs {name} and prepares its shared Sabine runtime.$\r$\n$\r$\nIf the runtime is not already available, setup downloads it before finishing. You can rerun this installer to repair the installation."
            ),
            payload: r"$PLUGINSDIR\payload",
            clear_previous: "",
            install_to: r#" --sabine-install-to "$INSTDIR""#,
            message: "Preparing the shared Sabine runtime...".to_string(),
        }
    }

    fn standalone(name: &str) -> Self {
        Self {
            welcome: format!(
                "Setup installs {name}. You can rerun this installer to repair the installation."
            ),
            payload: "$INSTDIR",
            clear_previous: "  RMDir /r \"$INSTDIR\\resources\\runtimes\"\n",
            install_to: "",
            message: format!("Checking {name}..."),
        }
    }
}

fn handler_commands(app: &BundleApp, name: &str, executable: &str) -> (String, String) {
    let command = format!(r#"'"$INSTDIR\{executable}" "%1"'"#);
    let mut register = String::new();
    let mut unregister = String::new();
    for scheme in app.associations.schemes() {
        register.push_str(&format!(
            r#"  WriteRegStr HKCU "Software\Classes\{scheme}" "" "URL:{scheme}"
  WriteRegStr HKCU "Software\Classes\{scheme}" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\{scheme}\shell\open\command" "" {command}
"#
        ));
        unregister.push_str(&format!(
            r#"  ReadRegStr $0 HKCU "Software\Classes\{scheme}\shell\open\command" ""
  StrCmp $0 {command} 0 +2
  DeleteRegKey HKCU "Software\Classes\{scheme}"
"#
        ));
    }
    if app.associations.extensions().next().is_none() {
        return (register, unregister);
    }
    let prog_id = prog_id(&app.id);
    register.push_str(&format!(
        r#"  WriteRegStr HKCU "Software\Classes\{prog_id}" "" "{name} document"
  WriteRegStr HKCU "Software\Classes\{prog_id}\DefaultIcon" "" '"$INSTDIR\{executable}",0'
  WriteRegStr HKCU "Software\Classes\{prog_id}\shell\open\command" "" {command}
"#
    ));
    unregister.push_str(&format!(
        r#"  ReadRegStr $0 HKCU "Software\Classes\{prog_id}\shell\open\command" ""
  StrCmp $0 {command} 0 document_types_kept
"#
    ));
    for extension in app.associations.extensions() {
        register.push_str(&format!(
            r#"  WriteRegStr HKCU "Software\Classes\.{extension}\OpenWithProgids" "{prog_id}" ""
"#
        ));
        unregister.push_str(&format!(
            r#"  DeleteRegValue HKCU "Software\Classes\.{extension}\OpenWithProgids" "{prog_id}"
"#
        ));
    }
    unregister.push_str(&format!(
        r#"  DeleteRegKey HKCU "Software\Classes\{prog_id}"
document_types_kept:
"#
    ));
    (register, unregister)
}

/// The links the Chromium an app ships lists for setup to make, relative to
/// the app's folder. The installers leave them out and remove them on
/// uninstall.
pub(super) fn runtime_links(app_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let resources = Path::new("resources");
    Ok(
        sabine_runtime::listed_runtime_links(&app_dir.join(resources))
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|link| resources.join(link))
            .collect(),
    )
}

fn uninstall_commands(root: &Path, directory: &Path, links: &[PathBuf]) -> Result<String, String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut commands = String::new();
    for link in links
        .iter()
        .filter(|link| root.join(link).parent() == Some(directory))
    {
        commands.push_str(&delete_command(link));
    }
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        if relative == Path::new(".sabine-install.json") || relative == Path::new("Uninstall.exe") {
            continue;
        }
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            commands.push_str(&uninstall_commands(root, &path, links)?);
            commands.push_str(&format!("  RMDir \"$INSTDIR\\{}\"\n", nsis_path(relative)));
        } else {
            commands.push_str(&delete_command(relative));
        }
    }
    Ok(commands)
}

fn delete_command(relative: &Path) -> String {
    format!(
        "  ClearErrors\n  Delete \"$INSTDIR\\{}\"\n  IfErrors unregister_failed\n",
        nsis_path(relative)
    )
}

fn nsis_path(relative: &Path) -> String {
    escape(&relative.to_string_lossy().replace('/', "\\"))
}

fn escape(value: &str) -> String {
    value
        .replace('$', "$$")
        .replace('"', "$\\\"")
        .replace('\r', "$\\r")
        .replace('\n', "$\\n")
}
