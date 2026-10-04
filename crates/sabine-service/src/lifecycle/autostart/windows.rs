// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Windows login startup uses an interactive, limited scheduled task targeting
// the GUI-subsystem daemon. Keep its battery and execution-time settings: the
// defaults can stop background maintenance long after a successful login.

use std::{path::Path, process::Stdio};

use sabine_runtime::background_command;

use super::run_checked;
use crate::{ServiceResult, windows_registry};

const TASK_NAME: &str = "Sabine Service";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Sabine";

pub(super) fn install(executable: &Path, daemon: &Path) -> ServiceResult<bool> {
    let daemon_literal = daemon.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$identity=[Security.Principal.WindowsIdentity]::GetCurrent();\
         $action=New-ScheduledTaskAction -Execute '{daemon_literal}';\
         $trigger=New-ScheduledTaskTrigger -AtLogOn -User $identity.Name;\
         $principal=New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Limited;\
         $settings=New-ScheduledTaskSettingsSet -Hidden -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew;\
         Register-ScheduledTask -TaskName '{TASK_NAME}' -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force | Out-Null"
    );
    run_checked(background_command("powershell.exe").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &script,
    ]))?;
    for (name, value) in [
        ("DisplayName", "Sabine".to_string()),
        ("DisplayVersion", crate::SABINE_VERSION.to_string()),
        ("Publisher", "Lantharos".to_string()),
        (
            "UninstallString",
            format!("\"{}\" uninstall", executable.display()),
        ),
    ] {
        windows_registry::set_current_user_value(UNINSTALL_KEY, name, &value)?;
    }
    Ok(false)
}

pub(super) fn uninstall() -> ServiceResult<()> {
    let _ = background_command("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    windows_registry::delete_current_user_key(UNINSTALL_KEY)?;
    Ok(())
}
