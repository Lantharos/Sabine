use serde::{Deserialize, Serialize};

use super::{MIN_SUPPORTED_APP_BUILD, SABINE_BUILD, SABINE_MAJOR};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SabineVersion {
    pub major: u32,
    pub build: u32,
}

impl SabineVersion {
    pub const fn current() -> Self {
        Self {
            major: SABINE_MAJOR,
            build: SABINE_BUILD,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let parts = value
            .trim_start_matches('v')
            .split('.')
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        match parts.as_slice() {
            [major, build] => Some(Self {
                major: *major,
                build: *build,
            }),
            [major, 1, build] if *build > 0 => Some(Self {
                major: *major,
                build: *build,
            }),
            [major, build, 0] => Some(Self {
                major: *major,
                build: *build,
            }),
            _ => None,
        }
    }

    pub fn label(self) -> String {
        format!("{}.{}", self.major, self.build)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SystemCompatibility {
    pub major: u32,
    pub build: u32,
    pub minimum_app_build: u32,
}

impl Default for SystemCompatibility {
    fn default() -> Self {
        Self {
            major: SABINE_MAJOR,
            build: 0,
            minimum_app_build: 0,
        }
    }
}

impl SystemCompatibility {
    pub const fn current() -> Self {
        Self {
            major: SABINE_MAJOR,
            build: SABINE_BUILD,
            minimum_app_build: MIN_SUPPORTED_APP_BUILD,
        }
    }

    pub fn accepts(self, app: SabineVersion) -> bool {
        app.build == 0
            || (app.major == self.major
                && app.build >= self.minimum_app_build
                && app.build <= self.build)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SystemReleaseManifest {
    pub schema: u32,
    pub version: String,
    pub published_at: String,
    #[serde(default)]
    pub compatibility: SystemCompatibility,
    pub artifacts: std::collections::BTreeMap<String, SystemReleaseArtifact>,
    #[serde(default)]
    pub signature: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SystemReleaseArtifact {
    pub sha256: String,
    pub size: u64,
    pub url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{SABINE_VERSION, version_is_newer};

    #[test]
    fn public_and_internal_versions_resolve_to_the_same_build() {
        let current = SabineVersion::current();
        assert_eq!(SabineVersion::parse(SABINE_VERSION), Some(current));
        assert_eq!(
            SabineVersion::parse(env!("CARGO_PKG_VERSION")),
            Some(current)
        );
        assert_eq!(
            SabineVersion::parse("0.1.20"),
            Some(SabineVersion {
                major: 0,
                build: 20
            })
        );
        assert!(version_is_newer("0.21", "0.1.20"));
    }

    #[test]
    fn compatibility_rejects_retired_and_future_app_builds() {
        let system = SystemCompatibility {
            major: 0,
            build: 21,
            minimum_app_build: 18,
        };
        assert!(system.accepts(SabineVersion {
            major: 0,
            build: 18
        }));
        assert!(!system.accepts(SabineVersion {
            major: 0,
            build: 17
        }));
        assert!(!system.accepts(SabineVersion {
            major: 0,
            build: 22
        }));
        assert!(!system.accepts(SabineVersion {
            major: 1,
            build: 18
        }));
    }
}
