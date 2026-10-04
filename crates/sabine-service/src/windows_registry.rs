use std::{
    io,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
            RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW,
            RegSetValueExW,
        },
    },
    core::HSTRING,
};

/// Reads a string value from `HKEY_CURRENT_USER\<key>`; an empty name reads the default value.
pub fn current_user_value(key: &str, name: &str) -> Option<String> {
    let key = HSTRING::from(key);
    let name = HSTRING::from(name);
    let mut size = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &name,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let mut buffer = vec![0u16; size as usize / 2];
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &key,
            &name,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then(|| {
        let length = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..length])
    })
}

pub fn current_user_string(key: &str) -> Option<String> {
    current_user_value(key, "")
}

/// Writes a string value under `HKEY_CURRENT_USER\<key>`, creating the key as needed.
pub fn set_current_user_value(key: &str, name: &str, value: &str) -> io::Result<()> {
    let mut handle = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            Some(0),
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut handle,
            None,
        )
    };
    check(status, key)?;
    let data = value.encode_utf16().chain([0]).collect::<Vec<_>>();
    let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
    let status =
        unsafe { RegSetValueExW(handle, &HSTRING::from(name), Some(0), REG_SZ, Some(bytes)) };
    unsafe {
        let _ = RegCloseKey(handle);
    }
    check(status, key)
}

pub fn delete_current_user_value(key: &str, name: &str) -> io::Result<()> {
    let status =
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name)) };
    check_removal(status, key)
}

pub fn delete_current_user_key(key: &str) -> io::Result<()> {
    let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key)) };
    check_removal(status, key)
}

/// Whether a path registered in the registry lies inside `root`, comparing the
/// way Windows does: without regard to case or separator style.
pub fn path_within(path: &Path, root: &Path) -> bool {
    folded(path).starts_with(folded(root))
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    folded(left) == folded(right)
}

fn folded(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

fn check(status: WIN32_ERROR, key: &str) -> io::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "could not write HKCU\\{key}: {status:?}"
        )))
    }
}

fn check_removal(status: WIN32_ERROR, key: &str) -> io::Result<()> {
    if matches!(
        status,
        ERROR_SUCCESS | ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND
    ) {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "could not remove HKCU\\{key}: {status:?}"
        )))
    }
}
