// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The login daemon must remain a Windows GUI-subsystem executable, even though
// it has no GUI. Launching the console service binary instead brings conhost
// along for the ride. Send failures to the daemon log, not an invisible console.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    if let Err(error) = sabine_service::run_daemon() {
        sabine_runtime::report_error("daemon", error);
        std::process::exit(1);
    }
}
