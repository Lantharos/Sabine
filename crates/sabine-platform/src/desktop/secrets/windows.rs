// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// The Credential Manager keeps generic credentials per user, under a target
// name unique for that user, with a blob of at most 2560 bytes.

use windows::Win32::Foundation::{ERROR_NOT_FOUND, FILETIME};
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

pub(super) async fn set(app_id: &str, key: &str, value: &str) -> Result<(), String> {
    if value.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
        return Err(format!(
            "Windows keeps secrets of up to {CRED_MAX_CREDENTIAL_BLOB_SIZE} bytes"
        ));
    }
    let target = target(app_id, key);
    let label = HSTRING::from(super::label(app_id, key));
    let mut blob = value.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_ptr().cast_mut()),
        Comment: PWSTR(label.as_ptr().cast_mut()),
        LastWritten: FILETIME::default(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: PWSTR::null(),
        UserName: PWSTR::null(),
    };
    unsafe { CredWriteW(&credential, 0) }.map_err(refused)
}

pub(super) async fn get(app_id: &str, key: &str) -> Result<Option<String>, String> {
    let target = target(app_id, key);
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    match unsafe {
        CredReadW(
            PCWSTR(target.as_ptr()),
            CRED_TYPE_GENERIC,
            None,
            &mut credential,
        )
    } {
        Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => return Ok(None),
        result => result.map_err(refused)?,
    }
    let value = unsafe {
        let credential = &*credential;
        std::slice::from_raw_parts(
            credential.CredentialBlob,
            credential.CredentialBlobSize as usize,
        )
        .to_vec()
    };
    unsafe { CredFree(credential.cast()) };
    String::from_utf8(value).map(Some).map_err(|_| {
        "The Credential Manager holds a secret under this key that is not text".to_string()
    })
}

pub(super) async fn delete(app_id: &str, key: &str) -> Result<bool, String> {
    let target = target(app_id, key);
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => Ok(false),
        result => result.map(|()| true).map_err(refused),
    }
}

fn target(app_id: &str, key: &str) -> HSTRING {
    HSTRING::from(format!("{app_id}/{key}"))
}

fn refused(error: windows::core::Error) -> String {
    format!("The Credential Manager refused: {}", error.message())
}
