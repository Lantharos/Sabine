use serde::Deserialize;

use super::{
    AppArtifactKind, AppInstallMode, AppUpdateConfig, AppUpdateSource, ServiceError, UpdatePolicy,
    app::stable_channel,
};

/// `[updates]` in `Sabine.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AppUpdatesSection {
    provider: Option<Provider>,
    repository: Option<String>,
    url: Option<String>,
    channel: Option<String>,
    policy: Option<PolicySetting>,
    public_key: Option<String>,
    install_mode: Option<AppInstallMode>,
    package_kind: Option<AppArtifactKind>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Provider {
    Github,
    Http,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum PolicySetting {
    Automatic,
    Notify,
    Disabled,
}

impl AppUpdatesSection {
    /// Where the app updates from, or `None` when its updates are disabled.
    pub fn resolve(self) -> Result<Option<AppUpdateConfig>, String> {
        let policy = match self.policy {
            Some(PolicySetting::Disabled) => return Ok(None),
            Some(PolicySetting::Notify) => UpdatePolicy::Notify,
            Some(PolicySetting::Automatic) | None => UpdatePolicy::Automatic,
        };
        let source = match (self.provider, self.repository, self.url) {
            (Some(Provider::Github), Some(repository), None) => {
                AppUpdateSource::Github { repository }
            }
            (Some(Provider::Http), None, Some(url)) => AppUpdateSource::Http { url },
            (Some(Provider::Github), _, _) => {
                return Err(
                    "[updates] with provider = \"github\" takes a repository and no url".into(),
                );
            }
            (Some(Provider::Http), _, _) => {
                return Err(
                    "[updates] with provider = \"http\" takes a url and no repository".into(),
                );
            }
            (None, _, _) => {
                return Err("[updates] needs a provider, unless its policy is \"disabled\"".into());
            }
        };
        let config = AppUpdateConfig {
            source,
            channel: self.channel.unwrap_or_else(stable_channel),
            policy,
            install_mode: self.install_mode.unwrap_or_default(),
            public_key: self.public_key.unwrap_or_default(),
            package_kind: self.package_kind,
        };
        config
            .source
            .manifest_url(&config.channel)
            .map_err(|error| match error {
                ServiceError::InvalidManifest(message) => format!("[updates]: {message}"),
                error => error.to_string(),
            })?;
        Ok(Some(config))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(text: &str) -> Result<Option<AppUpdateConfig>, String> {
        #[derive(Deserialize)]
        struct File {
            updates: AppUpdatesSection,
        }
        toml::from_str::<File>(text)
            .map_err(|error| error.to_string())?
            .updates
            .resolve()
    }

    #[test]
    fn disabled_turns_updates_off_even_with_a_source() {
        assert_eq!(resolve("[updates]\npolicy = \"disabled\""), Ok(None));
        assert_eq!(
            resolve(
                "[updates]\nprovider = \"github\"\nrepository = \"owner/app\"\npolicy = \"disabled\""
            ),
            Ok(None)
        );
    }

    #[test]
    fn enabled_policies_need_a_source() {
        assert!(resolve("[updates]\npolicy = \"notify\"").is_err());
        assert!(resolve("[updates]\nprovider = \"http\"\nrepository = \"owner/app\"").is_err());
    }

    #[test]
    fn github_sources_have_only_the_stable_channel() {
        assert!(
            resolve(
                "[updates]\nprovider = \"github\"\nrepository = \"owner/app\"\nchannel = \"beta\""
            )
            .is_err()
        );
    }

    #[test]
    fn reads_a_source() {
        let config = resolve(
            "[updates]\nprovider = \"github\"\nrepository = \"owner/app\"\npolicy = \"notify\"",
        )
        .unwrap()
        .unwrap();
        assert_eq!(config.policy, UpdatePolicy::Notify);
        assert_eq!(config.channel, "stable");
    }
}
