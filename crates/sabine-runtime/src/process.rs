// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// On Windows, redirecting stdout does not stop a console executable from
// opening conhost. Background helpers need CREATE_NO_WINDOW as well.
// Quiet output and an invisible process are annoyingly different things.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

pub fn background_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    configure_background_command(&mut command);
    command
}

pub fn configure_background_command(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

pub fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::{CloseHandle, STILL_ACTIVE},
            System::Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        };
        let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
        else {
            return false;
        };
        let mut exit_code = 0;
        let active = unsafe { GetExitCodeProcess(process, &mut exit_code) }.is_ok()
            && exit_code == STILL_ACTIVE.0 as u32;
        let _ = unsafe { CloseHandle(process) };
        active
    }
}

/// A process by its id and the moment it started, which together never repeat
/// even when the system hands the id to a later process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProcessIdentity {
    pub pid: u32,
    pub started: u64,
}

impl ProcessIdentity {
    pub fn current() -> Option<Self> {
        Self::of(std::process::id())
    }

    pub fn of(pid: u32) -> Option<Self> {
        process_alive(pid).then_some(())?;
        Some(Self {
            pid,
            started: process_start_time(pid)?,
        })
    }

    pub fn is_running(self) -> bool {
        Self::of(self.pid) == Some(self)
    }
}

#[cfg(target_os = "linux")]
fn process_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, fields) = stat.rsplit_once(')')?;
    fields.split_whitespace().nth(19)?.parse().ok()
}

#[cfg(target_os = "macos")]
fn process_start_time(pid: u32) -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let written = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    (written == size).then_some(())?;
    let info = unsafe { info.assume_init() };
    Some(info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec)
}

#[cfg(target_os = "windows")]
fn process_start_time(pid: u32) -> Option<u64> {
    use windows::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let result =
        unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) };
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;
    Some(u64::from(created.dwHighDateTime) << 32 | u64::from(created.dwLowDateTime))
}

/// Finds an executable on `PATH`, honoring `PATHEXT` on Windows.
pub fn find_program(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).find_map(|directory| {
        let candidate = directory.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(target_os = "windows")]
        if Path::new(name).extension().is_none() {
            return windows_extensions()
                .map(|extension| directory.join(format!("{name}{extension}")))
                .find(|candidate| is_executable(candidate));
        }
        None
    })
}

#[cfg(target_os = "windows")]
fn windows_extensions() -> impl Iterator<Item = String> {
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .into_iter()
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(windows)]
    {
        path.is_file()
    }
}
