use sabine_platform::{AutostartEntry, DeepLinkRegistration, NativeMessagingHost};
use windows::Win32::{
    Foundation::ERROR_SUCCESS,
    System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegSetValueExW,
    },
};
use windows::core::PCWSTR;

pub(in crate::desktop) fn write_autostart_entry(entry: &AutostartEntry) -> Result<(), String> {
    let key = sabine_service::APP_AUTOSTART_KEY;
    if entry.enabled {
        sabine_service::windows_registry::set_current_user_value(key, &entry.id, &entry.command)
    } else {
        sabine_service::windows_registry::delete_current_user_value(key, &entry.id)
    }
    .map_err(|error| error.to_string())
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
        set_registry_string(HKEY_CURRENT_USER, &base, "", &format!("URL:{scheme}"))?;
        set_registry_string(HKEY_CURRENT_USER, &base, "URL Protocol", "")?;
        set_registry_string(
            HKEY_CURRENT_USER,
            &format!("{base}\\shell\\open\\command"),
            "",
            &command,
        )?;
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
            set_registry_string(
                HKEY_CURRENT_USER,
                &format!(r"{key}\{}", host.id),
                "",
                &manifest.display().to_string(),
            )?;
        }
    }
    Ok(())
}

pub(super) fn set_registry_string(
    root: HKEY,
    subkey: &str,
    value_name: &str,
    data: &str,
) -> Result<(), String> {
    let subkey_wide = wide_null(subkey);
    let mut key = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            root,
            PCWSTR(subkey_wide.as_ptr()),
            Some(0),
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("RegCreateKeyExW failed: {status:?}"));
    }
    let value_wide = wide_null(value_name);
    let data_wide = wide_null(data);
    let bytes =
        unsafe { std::slice::from_raw_parts(data_wide.as_ptr() as *const u8, data_wide.len() * 2) };
    let result = unsafe {
        RegSetValueExW(
            key,
            PCWSTR(value_wide.as_ptr()),
            Some(0),
            REG_SZ,
            Some(bytes),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if result != ERROR_SUCCESS {
        return Err(format!("RegSetValueExW failed: {result:?}"));
    }
    Ok(())
}

pub(super) fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
