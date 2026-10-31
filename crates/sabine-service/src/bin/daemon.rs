// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The login daemon must remain a Windows GUI-subsystem executable, even though
// it has no GUI. Launching the console service binary instead brings conhost
// along for the ride. Send failures to the daemon log, not an invisible console.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    let uninstall = std::env::args().nth(1).as_deref() == Some("uninstall");
    let result = if uninstall {
        sabine_service::uninstall_with_prompt()
    } else {
        sabine_service::run_daemon()
    };
    if let Err(error) = result {
        sabine_runtime::report_error(if uninstall { "uninstall" } else { "daemon" }, error);
        std::process::exit(1);
    }
}
