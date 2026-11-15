use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFBoolean, CFData, CFDictionary, CFRetained, CFString, CFType};
use objc2_security::{
    SecCopyErrorMessageString, SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate,
    errSecDuplicateItem, errSecItemNotFound, errSecSuccess, kSecAttrAccount, kSecAttrLabel,
    kSecAttrService, kSecClass, kSecClassGenericPassword, kSecMatchLimit, kSecMatchLimitOne,
    kSecReturnData, kSecValueData,
};

pub(super) async fn set(app_id: &str, key: &str, value: &str) -> Result<(), String> {
    let (app_id, key, value) = (app_id.to_owned(), key.to_owned(), value.to_owned());
    blocking(move || {
        let data = CFData::from_bytes(value.as_bytes());
        let query = item(&app_id, &key, &[]);
        let update = dictionary(&[(unsafe { kSecValueData }, data.as_ref())]);
        let status = unsafe { SecItemUpdate(query.as_opaque(), update.as_opaque()) };
        if status != errSecItemNotFound {
            return checked(status);
        }
        let label = CFString::from_str(&super::label(&app_id, &key));
        let attributes = item(
            &app_id,
            &key,
            &[
                (unsafe { kSecAttrLabel }, label.as_ref()),
                (unsafe { kSecValueData }, data.as_ref()),
            ],
        );
        match unsafe { SecItemAdd(attributes.as_opaque(), ptr::null_mut()) } {
            status if status == errSecDuplicateItem => {
                checked(unsafe { SecItemUpdate(query.as_opaque(), update.as_opaque()) })
            }
            status => checked(status),
        }
    })
    .await
}

pub(super) async fn get(app_id: &str, key: &str) -> Result<Option<String>, String> {
    let (app_id, key) = (app_id.to_owned(), key.to_owned());
    blocking(move || {
        let query = item(
            &app_id,
            &key,
            &[
                (unsafe { kSecReturnData }, CFBoolean::new(true).as_ref()),
                (
                    unsafe { kSecMatchLimit },
                    unsafe { kSecMatchLimitOne }.as_ref(),
                ),
            ],
        );
        let mut result: *const CFType = ptr::null();
        match unsafe { SecItemCopyMatching(query.as_opaque(), &mut result) } {
            status if status == errSecItemNotFound => return Ok(None),
            status => checked(status)?,
        }
        let result = NonNull::new(result.cast_mut()).ok_or("The keychain returned no secret")?;
        let data = unsafe { CFRetained::from_raw(result) }
            .downcast::<CFData>()
            .map_err(|_| "The keychain returned a secret that is not data".to_string())?;
        String::from_utf8(data.to_vec())
            .map(Some)
            .map_err(|_| "The keychain holds a secret under this key that is not text".to_string())
    })
    .await
}

pub(super) async fn delete(app_id: &str, key: &str) -> Result<bool, String> {
    let (app_id, key) = (app_id.to_owned(), key.to_owned());
    blocking(move || {
        let query = item(&app_id, &key, &[]);
        match unsafe { SecItemDelete(query.as_opaque()) } {
            status if status == errSecItemNotFound => Ok(false),
            status => checked(status).map(|()| true),
        }
    })
    .await
}

/// Keychain calls wait for the person whenever the keychain asks them
/// something, so they run off the async runtime's threads.
async fn blocking<T: Send + 'static>(
    call: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(call)
        .await
        .map_err(|error| error.to_string())?
}

/// The app's generic password under `key` in the login keychain, with
/// `extra` attributes.
fn item(
    app_id: &str,
    key: &str,
    extra: &[(&CFString, &CFType)],
) -> CFRetained<CFDictionary<CFString, CFType>> {
    let service = CFString::from_str(app_id);
    let account = CFString::from_str(key);
    let mut pairs = vec![
        (
            unsafe { kSecClass },
            unsafe { kSecClassGenericPassword }.as_ref(),
        ),
        (unsafe { kSecAttrService }, service.as_ref()),
        (unsafe { kSecAttrAccount }, account.as_ref()),
    ];
    pairs.extend_from_slice(extra);
    dictionary(&pairs)
}

fn dictionary(pairs: &[(&CFString, &CFType)]) -> CFRetained<CFDictionary<CFString, CFType>> {
    let (keys, values): (Vec<_>, Vec<_>) = pairs.iter().copied().unzip();
    CFDictionary::from_slices(&keys, &values)
}

fn checked(status: i32) -> Result<(), String> {
    if status == errSecSuccess {
        return Ok(());
    }
    let message = unsafe { SecCopyErrorMessageString(status, ptr::null_mut()) }
        .map(|message| message.to_string())
        .unwrap_or_else(|| format!("error {status}"));
    Err(format!("The keychain refused: {message}"))
}
