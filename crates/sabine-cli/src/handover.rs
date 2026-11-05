use std::{path::PathBuf, process::Command};

use sabine_service::{CLI_PINNED_ENV, newer_installed_cli};

const ENTRY_ENV: &str = "SABINE_CLI_ENTRY";

/// Runs the CLI of the newest Sabine on this machine in place of this one, so
/// whichever `sabine` a person starts follows Sabine's updates and nothing has
/// to replace a program that may be running or that the system owns.
/// Uninstalling stays here, where the program being removed is.
pub fn to_newer_cli() {
    if cfg!(debug_assertions)
        || std::env::var_os(CLI_PINNED_ENV).is_some()
        || std::env::var_os(ENTRY_ENV).is_some()
        || std::env::args_os()
            .nth(1)
            .is_some_and(|command| command == "uninstall")
    {
        return;
    }
    let (Some(cli), Ok(entry)) = (newer_installed_cli(), std::env::current_exe()) else {
        return;
    };
    let mut command = Command::new(&cli);
    command
        .args(std::env::args_os().skip(1))
        .env(ENTRY_ENV, entry);
    let error = replace_process(command);
    eprintln!("sabine: could not start {}: {error}", cli.display());
}

#[cfg(unix)]
fn replace_process(mut command: Command) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    command.exec()
}

#[cfg(windows)]
fn replace_process(mut command: Command) -> std::io::Error {
    let _ = ctrlc::set_handler(|| {});
    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => error,
    }
}

/// The `sabine` the person started, which stays where it is while versions of
/// Sabine come and go.
pub fn entry_path() -> Result<PathBuf, String> {
    match std::env::var_os(ENTRY_ENV) {
        Some(entry) => Ok(PathBuf::from(entry)),
        None => std::env::current_exe().map_err(|error| error.to_string()),
    }
}

/// The `sabine` the person started, when it sits where only the system can
/// write, as with an operating system image or a package manager.
pub fn system_owned_entry() -> Option<PathBuf> {
    let entry = entry_path().ok()?;
    let writable = tempfile::tempfile_in(entry.parent()?).is_ok();
    (!writable).then_some(entry)
}
