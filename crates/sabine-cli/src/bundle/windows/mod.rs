mod artwork;
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
use std::{fs, path::Path};

pub(super) fn nsis_script(
    app: &BundleApp,
    source: &Path,
    executable: &str,
    output: &str,
    icon: Option<&str>,
    welcome_bitmap: Option<&Path>,
) -> Result<String, String> {
    let source = dunce::canonicalize(source).map_err(|error| error.to_string())?;
    let name = escape(&app.name);
    let id = escape(&app.id);
    let executable = escape(executable);
    let uninstall_files = uninstall_commands(&source, &source)?;
    sabine_service::write_install_inventory(&source, &app.id, &["Uninstall.exe"])
        .map_err(|error| error.to_string())?;
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
ShowInstDetails show
ShowUninstDetails show
Var SetupRunning
Var SetupCancelled
Var CancelHandle
Var CancelButton
Var ProgressBar
Var ProgressStart
{icon}
{welcome_bitmap}!define MUI_CUSTOMFUNCTION_ABORT CancelSetup
!define MUI_WELCOMEPAGE_TEXT "Setup installs {name} and prepares its shared Sabine runtime.$\r$\n$\r$\nIf the runtime is not already available, setup downloads it before finishing. You can rerun this installer to repair the installation."
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
  SetOutPath "$PLUGINSDIR\payload"
  File /r "{source}"
  WriteUninstaller "$PLUGINSDIR\payload\Uninstall.exe"
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
  DetailPrint "Preparing the shared Sabine runtime..."
  nsExec::ExecToLog '"$PLUGINSDIR\payload\{executable}" --sabine-install --sabine-install-to "$INSTDIR" --sabine-install-cancel "$PLUGINSDIR\cancel" --sabine-install-progress-bar $ProgressBar'
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

fn uninstall_commands(root: &Path, directory: &Path) -> Result<String, String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut commands = String::new();
    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        if relative == Path::new(".sabine-install.json") || relative == Path::new("Uninstall.exe") {
            continue;
        }
        let target = escape(&relative.to_string_lossy().replace('/', "\\"));
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            commands.push_str(&uninstall_commands(root, &path)?);
            commands.push_str(&format!("  RMDir \"$INSTDIR\\{target}\"\n"));
        } else {
            commands.push_str(&format!(
                "  ClearErrors\n  Delete \"$INSTDIR\\{target}\"\n  IfErrors unregister_failed\n"
            ));
        }
    }
    Ok(commands)
}

fn escape(value: &str) -> String {
    value
        .replace('$', "$$")
        .replace('"', "$\\\"")
        .replace('\r', "$\\r")
        .replace('\n', "$\\n")
}
