use sabine_service::{
    AppInstallMode, AppUpdateConfig, AppUpdateSource, AppUpdatesSection, UpdatePolicy,
};

/// Bundles built in GitHub Actions update from the repository's releases,
/// unless `Sabine.toml` says otherwise.
fn github_actions_updates() -> Option<AppUpdateConfig> {
    let repository = std::env::var("GITHUB_REPOSITORY").ok()?;
    Some(AppUpdateConfig {
        source: AppUpdateSource::Github { repository },
        channel: "stable".to_string(),
        policy: UpdatePolicy::Automatic,
        install_mode: AppInstallMode::Managed,
        public_key: String::new(),
        package_kind: None,
    })
}

pub(crate) fn resolve_updates(
    section: Option<AppUpdatesSection>,
) -> Result<Option<AppUpdateConfig>, String> {
    let updates = match section {
        Some(section) => section.resolve()?,
        None => github_actions_updates(),
    };
    let Some(mut update) = updates else {
        return Ok(None);
    };
    if let Ok(private_key) = std::env::var("SABINE_UPDATE_SIGNING_KEY")
        && !private_key.trim().is_empty()
    {
        let public_key = sabine_service::public_key_from_private(&private_key)
            .map_err(|error| error.to_string())?;
        if !update.public_key.trim().is_empty() && update.public_key.trim() != public_key {
            return Err("update public_key does not match SABINE_UPDATE_SIGNING_KEY".to_string());
        }
        update.public_key = public_key;
    }
    if std::env::var_os("GITHUB_ACTIONS").is_some() && update.public_key.trim().is_empty() {
        return Err("SABINE_UPDATE_SIGNING_KEY is required for release bundles".to_string());
    }
    Ok(Some(update))
}
