use std::collections::BTreeMap;

use serde::Deserialize;

use crate::Sandbox;

/// What a page may ask the person for, beyond what every page can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    Camera,
    Microphone,
    /// Recording a screen, a window, and the sound they play.
    Screen,
    Geolocation,
    Notifications,
    Midi,
    Clipboard,
    PointerLock,
    KeyboardLock,
    LocalFonts,
    WindowManagement,
    IdleDetection,
    StorageAccess,
    ProtectedMedia,
    FileSystem,
    Sensors,
    LocalNetwork,
    /// Starting several downloads without being asked each time.
    Downloads,
}

impl Permission {
    pub const ALL: [Self; 18] = [
        Self::Camera,
        Self::Microphone,
        Self::Screen,
        Self::Geolocation,
        Self::Notifications,
        Self::Midi,
        Self::Clipboard,
        Self::PointerLock,
        Self::KeyboardLock,
        Self::LocalFonts,
        Self::WindowManagement,
        Self::IdleDetection,
        Self::StorageAccess,
        Self::ProtectedMedia,
        Self::FileSystem,
        Self::Sensors,
        Self::LocalNetwork,
        Self::Downloads,
    ];

    /// The name `Sabine.toml`, pages and the browser host use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::Screen => "screen",
            Self::Geolocation => "geolocation",
            Self::Notifications => "notifications",
            Self::Midi => "midi",
            Self::Clipboard => "clipboard",
            Self::PointerLock => "pointer-lock",
            Self::KeyboardLock => "keyboard-lock",
            Self::LocalFonts => "local-fonts",
            Self::WindowManagement => "window-management",
            Self::IdleDetection => "idle-detection",
            Self::StorageAccess => "storage-access",
            Self::ProtectedMedia => "protected-media",
            Self::FileSystem => "file-system",
            Self::Sensors => "sensors",
            Self::LocalNetwork => "local-network",
            Self::Downloads => "downloads",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|permission| permission.as_str() == name)
    }
}

/// A permission the app declares, with why it needs it when it says so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredPermission {
    pub permission: Permission,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Declaration {
    Wanted(bool),
    Reason(String),
}

/// `[permissions]` as written in `Sabine.toml`: each permission the app's
/// pages may ask for, set to `true` or to the reason the app needs it.
#[derive(Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct PermissionsSection(BTreeMap<String, Declaration>);

impl PermissionsSection {
    /// The declared permissions. An app Sabine confines must also let its
    /// sandbox reach the devices its pages ask for.
    pub fn resolve(self, sandbox: Option<&Sandbox>) -> Result<Vec<DeclaredPermission>, String> {
        let mut declared = Vec::new();
        for (name, declaration) in self.0 {
            let permission = Permission::parse(&name).ok_or_else(|| {
                format!(
                    "[permissions] has `{name}`; use {}",
                    Permission::ALL.map(Permission::as_str).join(", ")
                )
            })?;
            let reason = match declaration {
                Declaration::Wanted(false) => continue,
                Declaration::Wanted(true) => None,
                Declaration::Reason(reason) => {
                    let reason = reason.trim().to_string();
                    if reason.is_empty() || reason.chars().any(char::is_control) {
                        return Err(format!(
                            "[permissions] {name} must be true or say in one line why the app needs it"
                        ));
                    }
                    Some(reason)
                }
            };
            if let Some(Sandbox::Confined {
                camera, microphone, ..
            }) = sandbox
            {
                let device = match permission {
                    Permission::Camera if !camera => Some("camera"),
                    Permission::Microphone if !microphone => Some("microphone"),
                    _ => None,
                };
                if let Some(device) = device {
                    return Err(format!(
                        "[permissions] {device} needs `{device} = true` in [sandbox], which keeps the app from the device otherwise"
                    ));
                }
            }
            declared.push(DeclaredPermission { permission, reason });
        }
        Ok(declared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(text: &str, sandbox: Option<&Sandbox>) -> Result<Vec<DeclaredPermission>, String> {
        toml::from_str::<PermissionsSection>(text)
            .map_err(|error| error.to_string())?
            .resolve(sandbox)
    }

    #[test]
    fn takes_permissions_with_and_without_reasons() {
        let declared = resolve(
            "camera = \"Scan receipts\"\ngeolocation = true\nmidi = false",
            None,
        )
        .unwrap();
        assert_eq!(
            declared,
            [
                DeclaredPermission {
                    permission: Permission::Camera,
                    reason: Some("Scan receipts".to_string()),
                },
                DeclaredPermission {
                    permission: Permission::Geolocation,
                    reason: None,
                },
            ]
        );
        assert!(resolve("bluetooth = true", None).is_err());
        assert!(resolve("camera = \" \"", None).is_err());
    }

    #[test]
    fn confined_apps_declare_the_devices_their_pages_use() {
        let sandbox = Sandbox::Confined {
            network: true,
            filesystems: Vec::new(),
            devices: Vec::new(),
            talk: Vec::new(),
            own: Vec::new(),
            pipewire: false,
            x11: false,
            microphone: true,
            camera: false,
            record_windows: false,
        };
        assert!(resolve("microphone = true", Some(&sandbox)).is_ok());
        assert!(resolve("camera = true", Some(&sandbox)).is_err());
    }
}
