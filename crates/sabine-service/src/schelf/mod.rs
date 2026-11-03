//! Apps that come from schelf.store: its update feeds, signed by store keys
//! the built-in root vouches for, its rollouts, and its revocation list.

mod keys;
mod notices;
mod revocations;
mod rollout;
#[cfg(target_os = "linux")]
mod store;

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use ed25519_dalek::VerifyingKey;

use crate::{AppReleaseManifest, ServiceResult};
use keys::{Envelope, StoreKeys, untrusted};
pub use notices::StoreNotice;
pub(crate) use notices::installed_by_store;
use revocations::Revocations;

const DOWNLOAD_ORIGIN: &str = "https://dl.schelf.store";
const STORE_KEYS: &str = "trust/v1/store-keys.json";
const REVOCATIONS: &str = "trust/v1/revocations.json";
const STORE_KEYS_REFRESH: Duration = Duration::from_secs(24 * 60 * 60);
pub(crate) const REVOCATIONS_REFRESH: Duration = Duration::from_secs(60 * 60);
const SIGNED_FILE_LIMIT: u64 = 4 * 1024 * 1024;
/// How long an app's launch waits for a stale revocation list.
const LAUNCH_FETCH_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn feed_url(app_id: &str, channel: &str) -> String {
    format!("{DOWNLOAD_ORIGIN}/sabine/{app_id}/{channel}.json")
}

/// Where to download an update artifact from, telling Schelf the download is
/// an update so it counts it as one.
pub(crate) fn update_download_url(url: &str) -> String {
    if !url.starts_with(&format!("{DOWNLOAD_ORIGIN}/")) {
        return url.to_string();
    }
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}reason=update")
}

/// What Sabine keeps of Schelf's signed files, under `directory`.
pub(crate) struct Trust {
    directory: PathBuf,
    root: VerifyingKey,
    fetch: Fetch,
}

#[derive(Clone, Copy)]
enum Fetch {
    Patient,
    /// One quick try, while an app waits to open.
    Quick,
}

impl Trust {
    pub fn new(sabine_root: &Path) -> Self {
        Self {
            directory: sabine_root.join("schelf"),
            root: keys::root_key(),
            fetch: Fetch::Patient,
        }
    }

    pub fn at_launch(sabine_root: &Path) -> Self {
        Self {
            fetch: Fetch::Quick,
            ..Self::new(sabine_root)
        }
    }

    /// Checks that a current store key signed the update feed `release`.
    pub fn verify_feed(&self, release: &AppReleaseManifest) -> ServiceResult<()> {
        if self.store_keys(false)?.signed_feed(release)
            || self.store_keys(true)?.signed_feed(release)
        {
            Ok(())
        } else {
            Err(untrusted(
                "the update feed isn't signed by a current store key",
            ))
        }
    }

    /// Whether this installation takes a release offered to `percent` of them.
    pub fn in_rollout(&self, percent: Option<u8>) -> bool {
        rollout::includes(rollout::bucket(&self.directory), percent)
    }

    /// The revocation list, fetched again when the copy here is older than
    /// `max_age`. A list that can't be fetched or verified leaves the copy
    /// here in place.
    pub fn revocations(&self, max_age: Duration) -> ServiceResult<Option<Revocations>> {
        let seen = self.cached_revocations();
        if seen.is_some() && self.age(REVOCATIONS) < max_age {
            return Ok(seen);
        }
        match self.fetch_revocations(seen.as_ref()) {
            Ok(list) => Ok(Some(list)),
            Err(error) if seen.is_some() => {
                sabine_runtime::report_error("schelf", &error);
                Ok(seen)
            }
            Err(error) => Err(error),
        }
    }

    fn fetch_revocations(&self, seen: Option<&Revocations>) -> ServiceResult<Revocations> {
        let file = SignedFile::fetch(REVOCATIONS, self.fetch)?;
        if !self.store_keys(false)?.signed(&file.body, &file.envelope)
            && !self.store_keys(true)?.signed(&file.body, &file.envelope)
        {
            return Err(untrusted(
                "the revocation list isn't signed by a current store key",
            ));
        }
        let list = Revocations::parse(&file.body)?;
        list.succeeds(seen)?;
        file.save(&self.directory, REVOCATIONS)?;
        Ok(list)
    }

    fn cached_revocations(&self) -> Option<Revocations> {
        let file = SignedFile::load(&self.directory, REVOCATIONS)?;
        Revocations::parse(&file.body).ok()
    }

    /// The current store keys, fetched again when `refresh` or when the copy
    /// here is a day old.
    fn store_keys(&self, refresh: bool) -> ServiceResult<StoreKeys> {
        let cached = SignedFile::load(&self.directory, STORE_KEYS).and_then(|file| {
            StoreKeys::verify(&self.root, &file.body, &file.envelope, SystemTime::now()).ok()
        });
        let fresh = !refresh && self.age(STORE_KEYS) < STORE_KEYS_REFRESH;
        match cached {
            Some(keys) if fresh => Ok(keys),
            cached => self
                .fetch_store_keys(cached.as_ref())
                .or_else(|error| cached.ok_or(error)),
        }
    }

    fn fetch_store_keys(&self, cached: Option<&StoreKeys>) -> ServiceResult<StoreKeys> {
        let file = SignedFile::fetch(STORE_KEYS, self.fetch)?;
        let keys = StoreKeys::verify(&self.root, &file.body, &file.envelope, SystemTime::now())?;
        if cached.is_some_and(|cached| keys.issued < cached.issued) {
            return Err(untrusted(
                "store-keys.json is older than the copy already seen",
            ));
        }
        file.save(&self.directory, STORE_KEYS)?;
        Ok(keys)
    }

    fn age(&self, path: &str) -> Duration {
        std::fs::metadata(SignedFile::local_path(&self.directory, path))
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .unwrap_or(Duration::MAX)
    }
}

/// A file Schelf signs, and the envelope that signs it.
struct SignedFile {
    body: Vec<u8>,
    envelope: Envelope,
}

impl SignedFile {
    fn fetch(path: &str, fetch: Fetch) -> ServiceResult<Self> {
        let fetch = |path: &str| {
            let url = format!("{DOWNLOAD_ORIGIN}/{path}");
            match fetch {
                Fetch::Patient => sabine_runtime::fetch_bytes(&url, SIGNED_FILE_LIMIT),
                Fetch::Quick => {
                    sabine_runtime::fetch_bytes_once(&url, SIGNED_FILE_LIMIT, LAUNCH_FETCH_TIMEOUT)
                }
            }
            .map_err(|error| untrusted(&format!("could not fetch {path}: {error}")))
        };
        let envelope = serde_json::from_slice(&fetch(&format!("{path}.sig"))?)
            .map_err(|error| untrusted(&format!("{path}.sig is invalid: {error}")))?;
        Ok(Self {
            body: fetch(path)?,
            envelope,
        })
    }

    fn load(directory: &Path, path: &str) -> Option<Self> {
        let local = Self::local_path(directory, path);
        let envelope = std::fs::read(sig_path(&local)).ok()?;
        Some(Self {
            body: std::fs::read(local).ok()?,
            envelope: serde_json::from_slice(&envelope).ok()?,
        })
    }

    fn save(&self, directory: &Path, path: &str) -> ServiceResult<()> {
        let local = Self::local_path(directory, path);
        std::fs::create_dir_all(directory)?;
        sabine_runtime::write_json_atomic(&sig_path(&local), &self.envelope)?;
        let temporary = local.with_extension("download");
        std::fs::write(&temporary, &self.body)?;
        sabine_runtime::replace_file(&temporary, &local)?;
        Ok(())
    }

    fn local_path(directory: &Path, path: &str) -> PathBuf {
        directory.join(path.rsplit('/').next().expect("paths name a file"))
    }
}

fn sig_path(local: &Path) -> PathBuf {
    let mut name = local.as_os_str().to_owned();
    name.push(".sig");
    name.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feeds_follow_the_app_and_channel() {
        assert_eq!(
            feed_url("com.example.tern", "stable"),
            "https://dl.schelf.store/sabine/com.example.tern/stable.json"
        );
        assert_eq!(
            crate::AppUpdateSource::Schelf
                .manifest_url("com.example.tern", "beta")
                .unwrap(),
            "https://dl.schelf.store/sabine/com.example.tern/beta.json"
        );
        assert!(
            crate::AppUpdateSource::Schelf
                .manifest_url("com.example.tern", "../../trust/v1/store-keys")
                .is_err()
        );
    }

    #[test]
    fn update_downloads_from_schelf_say_so() {
        assert_eq!(
            update_download_url("https://dl.schelf.store/apps/a/1.0.0/linux-x86_64/a.tar.gz"),
            "https://dl.schelf.store/apps/a/1.0.0/linux-x86_64/a.tar.gz?reason=update"
        );
        assert_eq!(
            update_download_url("https://dl.schelf.store/apps/a.tar.gz?part=1"),
            "https://dl.schelf.store/apps/a.tar.gz?part=1&reason=update"
        );
        assert_eq!(
            update_download_url("https://github.com/a/b/releases/download/v1/a.tar.gz"),
            "https://github.com/a/b/releases/download/v1/a.tar.gz"
        );
    }
}
