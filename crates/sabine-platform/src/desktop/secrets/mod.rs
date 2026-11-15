//! Secrets an app keeps in the system's credential store, each under a key
//! of the app's own: the Secret Service on Linux, the login keychain on
//! macOS and the Credential Manager on Windows.

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

const MAX_KEY_LENGTH: usize = 256;

/// Keeps `value` under `key` for the app, replacing what the key held.
pub async fn set(app_id: &str, key: &str, value: &str) -> Result<(), String> {
    check_key(key)?;
    platform::set(app_id, key, value).await
}

/// The value the app keeps under `key`, or `None` when it keeps none.
pub async fn get(app_id: &str, key: &str) -> Result<Option<String>, String> {
    check_key(key)?;
    platform::get(app_id, key).await
}

/// Forgets what the app keeps under `key`. Returns whether there was a value.
pub async fn delete(app_id: &str, key: &str) -> Result<bool, String> {
    check_key(key)?;
    platform::delete(app_id, key).await
}

fn check_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > MAX_KEY_LENGTH || key.chars().any(char::is_control) {
        return Err(format!(
            "A secret's key is 1 to {MAX_KEY_LENGTH} bytes of text without control characters"
        ));
    }
    Ok(())
}

/// The label the person sees for a secret in their keyring or keychain.
fn label(app_id: &str, key: &str) -> String {
    format!("{key} ({app_id})")
}
