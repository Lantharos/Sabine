use crate::AppEnvironment;

/// The app's secrets, such as tokens and passwords, in the system's
/// credential store: the Secret Service on Linux, the login keychain on macOS
/// and the Credential Manager on Windows. Each one is text under a key of the
/// app's own.
#[derive(Clone, Debug)]
pub struct Secrets {
    app_id: String,
}

impl Secrets {
    /// The secrets of the app with `app_id`, as `Sabine.toml` names it.
    /// Development runs keep theirs under the development app id.
    pub fn new(app_id: &str) -> Self {
        Self {
            app_id: AppEnvironment::current().app_id(app_id),
        }
    }

    /// Keeps `value` under `key`, replacing what the key held.
    pub async fn set(&self, key: &str, value: &str) -> Result<(), String> {
        sabine_platform::secrets::set(&self.app_id, key, value).await
    }

    /// The value under `key`, or `None` when the app keeps none there.
    pub async fn get(&self, key: &str) -> Result<Option<String>, String> {
        sabine_platform::secrets::get(&self.app_id, key).await
    }

    /// Forgets the value under `key`. Returns whether there was one.
    pub async fn delete(&self, key: &str) -> Result<bool, String> {
        sabine_platform::secrets::delete(&self.app_id, key).await
    }
}
