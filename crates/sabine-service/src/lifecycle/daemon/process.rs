use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::ServiceResult;

pub(super) fn same_executable(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(target_os = "linux")]
pub(super) fn process_executable(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(target_os = "macos")]
pub(super) fn process_executable(pid: u32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let mut buffer = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length = unsafe {
        libc::proc_pidpath(
            pid as libc::c_int,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
        )
    };
    (length > 0).then(|| PathBuf::from(std::ffi::OsStr::from_bytes(&buffer[..length as usize])))
}

#[cfg(windows)]
pub(super) fn process_executable(pid: u32) -> Option<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows::{
        Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        core::PWSTR,
    };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result
        .ok()
        .map(|_| PathBuf::from(OsString::from_wide(&buffer[..length as usize])))
}

#[cfg(unix)]
pub(super) fn terminate_process(pid: u32) -> ServiceResult<()> {
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

#[cfg(windows)]
pub(super) fn terminate_process(pid: u32) -> ServiceResult<()> {
    use crate::ServiceError;
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
    };
    let process = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }
        .map_err(|error| ServiceError::Update(error.to_string()))?;
    let result = unsafe { TerminateProcess(process, 0) };
    let _ = unsafe { CloseHandle(process) };
    result.map_err(|error| ServiceError::Update(error.to_string()))
}
