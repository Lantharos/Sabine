use serde::Deserialize;

use super::keys::untrusted;
use crate::ServiceResult;

/// Schelf's list of blocked apps and releases.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Revocations {
    pub serial: u64,
    #[serde(default)]
    blocked: Vec<Blocked>,
}

#[derive(Clone, Debug, Deserialize)]
struct Blocked {
    app: String,
    versions: Versions,
    action: BlockAction,
    reason: String,
    note: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum Versions {
    Listed(Vec<String>),
    Every(String),
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum BlockAction {
    Warn,
    Block,
}

/// What Schelf says about one version of an app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Revocation {
    pub action: BlockAction,
    /// Why, in Schelf's words.
    pub explanation: String,
}

impl Revocations {
    pub fn parse(body: &[u8]) -> ServiceResult<Self> {
        serde_json::from_slice(body)
            .map_err(|error| untrusted(&format!("revocations.json is invalid: {error}")))
    }

    /// Takes `self` over the list already seen, which it may only follow.
    pub fn succeeds(&self, seen: Option<&Self>) -> ServiceResult<()> {
        match seen {
            Some(seen) if self.serial < seen.serial => Err(untrusted(&format!(
                "the revocation list went back from serial {} to {}",
                seen.serial, self.serial
            ))),
            _ => Ok(()),
        }
    }

    /// The strictest entry for `version` of `app`.
    pub fn find(&self, app: &str, version: &str) -> Option<Revocation> {
        self.blocked
            .iter()
            .filter(|entry| entry.app.eq_ignore_ascii_case(app) && entry.versions.cover(version))
            .max_by_key(|entry| entry.action)
            .map(|entry| Revocation {
                action: entry.action,
                explanation: entry.note.clone().unwrap_or_else(|| entry.reason.clone()),
            })
    }
}

impl Versions {
    fn cover(&self, version: &str) -> bool {
        match self {
            Self::Listed(versions) => versions.iter().any(|listed| listed == version),
            Self::Every(pattern) => pattern == "*",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(serial: u64) -> Revocations {
        Revocations::parse(
            format!(
                r#"{{
                    "serial": {serial},
                    "issued": "2026-11-02T10:00:00Z",
                    "blocked": [
                        {{ "app": "com.example.Bad", "versions": "*", "action": "block", "reason": "malware", "note": "Sends your files to a third party." }},
                        {{ "app": "io.github.someone.tool", "versions": ["2.1.0"], "action": "warn", "reason": "crashes on start" }},
                        {{ "app": "io.github.someone.tool", "versions": ["2.1.0", "2.2.0"], "action": "block", "reason": "deletes settings" }}
                    ]
                }}"#
            )
            .as_bytes(),
        )
        .unwrap()
    }

    #[test]
    fn never_goes_back_to_an_older_list() {
        assert!(list(41).succeeds(None).is_ok());
        assert!(list(41).succeeds(Some(&list(41))).is_ok());
        assert!(list(42).succeeds(Some(&list(41))).is_ok());
        assert!(list(40).succeeds(Some(&list(41))).is_err());
    }

    #[test]
    fn finds_the_strictest_entry_for_a_version() {
        let list = list(1);
        assert_eq!(
            list.find("com.example.bad", "9.9.9"),
            Some(Revocation {
                action: BlockAction::Block,
                explanation: "Sends your files to a third party.".into()
            })
        );
        assert_eq!(
            list.find("io.github.someone.tool", "2.1.0").unwrap(),
            Revocation {
                action: BlockAction::Block,
                explanation: "deletes settings".into()
            }
        );
        assert_eq!(list.find("io.github.someone.tool", "2.0.0"), None);
        assert_eq!(list.find("io.github.someone.toolkit", "2.1.0"), None);
    }
}
