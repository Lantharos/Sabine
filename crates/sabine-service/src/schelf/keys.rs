use std::{collections::BTreeMap, time::SystemTime};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::release::signing::{app_release_signed_by, decode_array};
use crate::{AppReleaseManifest, ServiceError, ServiceResult};

const ROOT_KEY: &str = include_str!("../../schelf-root-key.txt");

/// The detached signature beside a signed file, `x.json.sig`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Envelope {
    pub key: String,
    pub sig: String,
}

impl Envelope {
    fn verifies(&self, key: &VerifyingKey, body: &[u8]) -> bool {
        decode_array::<64>(&self.sig, "signature")
            .is_ok_and(|signature| key.verify(body, &Signature::from_bytes(&signature)).is_ok())
    }
}

#[derive(Deserialize)]
struct StoreKeysFile {
    version: u32,
    issued: String,
    keys: Vec<StoreKeyEntry>,
    #[serde(default)]
    revoked: Vec<String>,
}

#[derive(Deserialize)]
struct StoreKeyEntry {
    id: String,
    public_key: String,
    not_before: String,
    expires: String,
}

/// The store signing keys the root vouches for at one moment.
pub(crate) struct StoreKeys {
    pub issued: SystemTime,
    keys: BTreeMap<String, VerifyingKey>,
}

impl StoreKeys {
    /// Reads `store-keys.json`, keeping the keys that `root` signed, that
    /// aren't revoked and that are valid at `now`.
    pub fn verify(
        root: &VerifyingKey,
        body: &[u8],
        envelope: &Envelope,
        now: SystemTime,
    ) -> ServiceResult<Self> {
        if envelope.key != key_id(root) || !envelope.verifies(root, body) {
            return Err(untrusted(
                "store-keys.json isn't signed by the Schelf root key",
            ));
        }
        let file: StoreKeysFile = serde_json::from_slice(body)
            .map_err(|error| untrusted(&format!("store-keys.json is invalid: {error}")))?;
        if file.version != 1 {
            return Err(untrusted(&format!(
                "store-keys.json has unsupported version {}",
                file.version
            )));
        }
        let mut keys = BTreeMap::new();
        for entry in file.keys {
            if file.revoked.contains(&entry.id)
                || timestamp(&entry.not_before)? > now
                || timestamp(&entry.expires)? <= now
            {
                continue;
            }
            let key = verifying_key(&entry.public_key)?;
            if key_id(&key) != entry.id {
                return Err(untrusted(&format!(
                    "store key {} doesn't match its public key",
                    entry.id
                )));
            }
            keys.insert(entry.id, key);
        }
        Ok(Self {
            issued: timestamp(&file.issued)?,
            keys,
        })
    }

    /// Whether a current store key signed `body`, as `envelope` says.
    pub fn signed(&self, body: &[u8], envelope: &Envelope) -> bool {
        self.keys
            .get(&envelope.key)
            .is_some_and(|key| envelope.verifies(key, body))
    }

    /// Whether a current store key signed the update feed `release`: the one
    /// it names, or any of them when it names none.
    pub fn signed_feed(&self, release: &AppReleaseManifest) -> bool {
        match &release.key {
            Some(id) => self
                .keys
                .get(id)
                .is_some_and(|key| app_release_signed_by(release, key)),
            None => self
                .keys
                .values()
                .any(|key| app_release_signed_by(release, key)),
        }
    }
}

pub(crate) fn root_key() -> VerifyingKey {
    verifying_key(ROOT_KEY).expect("the built-in Schelf root key is valid")
}

/// A key's id: the first 16 hex digits of the SHA-256 of its public key.
pub(crate) fn key_id(key: &VerifyingKey) -> String {
    Sha256::digest(key.as_bytes())[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn timestamp(value: &str) -> ServiceResult<SystemTime> {
    humantime::parse_rfc3339(value)
        .map_err(|error| untrusted(&format!("invalid timestamp {value}: {error}")))
}

fn verifying_key(encoded: &str) -> ServiceResult<VerifyingKey> {
    VerifyingKey::from_bytes(&decode_array::<32>(encoded, "store public key")?)
        .map_err(|_| untrusted("a Schelf public key is invalid"))
}

pub(crate) fn untrusted(message: &str) -> ServiceError {
    ServiceError::Update(format!("Schelf: {message}"))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn envelope(key: &SigningKey, body: &[u8]) -> Envelope {
        Envelope {
            key: key_id(&key.verifying_key()),
            sig: STANDARD.encode(key.sign(body).to_bytes()),
        }
    }

    fn store_keys_body(store: &SigningKey, revoked: &[&str]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "issued": "2026-11-03T10:00:00.000Z",
            "keys": [{
                "id": key_id(&store.verifying_key()),
                "public_key": STANDARD.encode(store.verifying_key().as_bytes()),
                "not_before": "2026-11-01T00:00:00.000Z",
                "expires": "2027-11-01T00:00:00Z",
            }],
            "revoked": revoked,
        }))
        .unwrap()
    }

    fn now() -> SystemTime {
        timestamp("2026-11-03T12:00:00Z").unwrap()
    }

    fn feed() -> AppReleaseManifest {
        serde_json::from_value(serde_json::json!({
            "app_id": "com.example.tern",
            "version": "1.1.0",
            "published_at": "1793491200",
            "artifacts": {},
            "rollout": 25,
        }))
        .unwrap()
    }

    fn sign_feed(release: &mut AppReleaseManifest, key: &SigningKey) {
        crate::sign_app_release(release, &STANDARD.encode(key.to_bytes())).unwrap();
    }

    #[test]
    fn trusts_store_keys_only_through_the_root() {
        let root = signing_key(1);
        let store = signing_key(2);
        let body = store_keys_body(&store, &[]);
        let keys = StoreKeys::verify(&root.verifying_key(), &body, &envelope(&root, &body), now())
            .unwrap();
        let mut release = feed();
        sign_feed(&mut release, &store);
        assert!(keys.signed_feed(&release));

        let impostor = signing_key(3);
        assert!(
            StoreKeys::verify(
                &root.verifying_key(),
                &body,
                &envelope(&impostor, &body),
                now()
            )
            .is_err()
        );
        let mut tampered = body.clone();
        tampered.extend_from_slice(b" ");
        assert!(
            StoreKeys::verify(
                &root.verifying_key(),
                &tampered,
                &envelope(&root, &body),
                now()
            )
            .is_err()
        );
    }

    #[test]
    fn drops_revoked_and_expired_store_keys() {
        let root = signing_key(1);
        let store = signing_key(2);
        let mut release = feed();
        sign_feed(&mut release, &store);

        let revoked = store_keys_body(&store, &[&key_id(&store.verifying_key())]);
        let keys = StoreKeys::verify(
            &root.verifying_key(),
            &revoked,
            &envelope(&root, &revoked),
            now(),
        )
        .unwrap();
        assert!(!keys.signed_feed(&release));

        let body = store_keys_body(&store, &[]);
        let later = now() + Duration::from_secs(400 * 24 * 60 * 60);
        let keys = StoreKeys::verify(&root.verifying_key(), &body, &envelope(&root, &body), later)
            .unwrap();
        assert!(!keys.signed_feed(&release));
    }

    #[test]
    fn feeds_name_the_key_that_signed_them() {
        let root = signing_key(1);
        let store = signing_key(2);
        let body = store_keys_body(&store, &[]);
        let keys = StoreKeys::verify(&root.verifying_key(), &body, &envelope(&root, &body), now())
            .unwrap();
        let mut release = feed();
        release.key = Some(key_id(&store.verifying_key()));
        sign_feed(&mut release, &store);
        assert!(keys.signed_feed(&release));

        release.key = Some("0000000000000000".into());
        sign_feed(&mut release, &store);
        assert!(!keys.signed_feed(&release));

        release.key = None;
        release.rollout = Some(100);
        assert!(!keys.signed_feed(&release));
    }

    #[test]
    fn key_ids_match_the_stores() {
        assert_eq!(key_id(&root_key()), "02753446e2b2ff40");
    }
}
