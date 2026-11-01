use sabine_platform::{AutostartEntry, DeepLinkRegistration, NativeMessagingHost};
use sabine_service::windows_registry::{
    current_user_value, delete_current_user_value, set_current_user_value,
};

/// Turning the app off under Startup apps keeps its `Run` value and marks it
/// in `StartupApproved`, so rewriting a changed command leaves that choice.
pub(in crate::desktop) fn write_autostart_entry(entry: &AutostartEntry) -> Result<(), String> {
    let key = sabine_service::APP_AUTOSTART_KEY;
    if !entry.enabled {
        return delete_current_user_value(key, &entry.id).map_err(|error| error.to_string());
    }
    if current_user_value(key, &entry.id).as_deref() == Some(entry.command.as_str()) {
        return Ok(());
    }
    set_current_user_value(key, &entry.id, &entry.command).map_err(|error| error.to_string())
}

pub(in crate::desktop) fn register_deep_links(
    registration: &DeepLinkRegistration,
) -> Result<(), String> {
    registration.validate()?;
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let command = format!("\"{}\" \"%1\"", exe.display());
    for scheme in &registration.schemes {
        let scheme = scheme.to_ascii_lowercase();
        let base = format!("Software\\Classes\\{scheme}");
        set_string(&base, "", &format!("URL:{scheme}"))?;
        set_string(&base, "URL Protocol", "")?;
        set_string(&format!("{base}\\shell\\open\\command"), "", &command)?;
    }
    Ok(())
}

pub(in crate::desktop) fn register_native_messaging_host(
    host: &NativeMessagingHost,
) -> Result<(), String> {
    let manifests = crate::desktop::native_messaging::write_manifests(host)
        .map_err(|error| error.to_string())?;
    for (browser, manifest) in manifests {
        for key in sabine_service::native_messaging_registry_keys(browser) {
            set_string(
                &format!(r"{key}\{}", host.id),
                "",
                &manifest.display().to_string(),
            )?;
        }
    }
    Ok(())
}

fn set_string(key: &str, name: &str, value: &str) -> Result<(), String> {
    set_current_user_value(key, name, value).map_err(|error| error.to_string())
}
