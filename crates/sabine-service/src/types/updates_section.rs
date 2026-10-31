use serde::Deserialize;

use super::AppUpdateConfig;

/// `[updates]` in `Sabine.toml`: where the app updates from, or only
/// `policy = "disabled"` for an app that never updates itself.
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "toml::Table")]
pub enum AppUpdatesSection {
    Source(AppUpdateConfig),
    Disabled,
}

impl AppUpdatesSection {
    /// The update source, or `None` for an app that never updates.
    pub fn source(self) -> Option<AppUpdateConfig> {
        match self {
            Self::Source(config) => Some(config),
            Self::Disabled => None,
        }
    }
}

impl TryFrom<toml::Table> for AppUpdatesSection {
    type Error = String;

    fn try_from(table: toml::Table) -> Result<Self, String> {
        if table.contains_key("provider") {
            return toml::Value::Table(table)
                .try_into()
                .map(Self::Source)
                .map_err(|error| error.to_string());
        }
        match table.get("policy").and_then(toml::Value::as_str) {
            Some("disabled") if table.len() == 1 => Ok(Self::Disabled),
            _ => {
                Err("[updates] needs a provider, unless it holds only policy = \"disabled\"".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UpdatePolicy;

    #[derive(Deserialize)]
    struct File {
        updates: AppUpdatesSection,
    }

    fn section(text: &str) -> Result<AppUpdatesSection, toml::de::Error> {
        toml::from_str::<File>(text).map(|file| file.updates)
    }

    #[test]
    fn disabled_needs_no_source() {
        let disabled = section("[updates]\npolicy = \"disabled\"").unwrap();
        assert!(disabled.source().is_none());
    }

    #[test]
    fn other_policies_need_a_source() {
        assert!(section("[updates]\npolicy = \"notify\"").is_err());
        assert!(section("[updates]\npolicy = \"disabled\"\nchannel = \"beta\"").is_err());
    }

    #[test]
    fn reads_a_source() {
        let source = section(
            "[updates]\nprovider = \"github\"\nrepository = \"owner/app\"\npolicy = \"notify\"",
        )
        .unwrap()
        .source()
        .unwrap();
        assert_eq!(source.policy, UpdatePolicy::Notify);
    }
}
