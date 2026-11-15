use std::{fs::File, io::Read, path::Path};

use sabine_service::{DeclaredPermission, Permission};

use crate::util::xml;

pub fn validate_executable(path: &Path) -> Result<(), String> {
    let mut header = [0u8; 32];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    let magic = u32::from_le_bytes(header[0..4].try_into().unwrap());
    let cpu = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let kind = u32::from_le_bytes(header[12..16].try_into().unwrap());
    if magic != 0xfeed_facf || cpu != 0x0100_000c || kind != 2 {
        return Err(format!(
            "macOS bundles require an Apple Silicon Mach-O executable; rebuild {} for aarch64-apple-darwin",
            path.display()
        ));
    }
    Ok(())
}

/// What an app's `Info.plist` describes.
pub struct PlistApp<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub version: &'a str,
    pub executable: &'a str,
    pub has_icon: bool,
    pub associations: &'a crate::desktop::types::Associations,
    pub listing: &'a crate::desktop::entry::Listing,
    pub permissions: &'a [DeclaredPermission],
}

pub fn info_plist(app: &PlistApp) -> Result<String, String> {
    let PlistApp {
        id,
        name,
        version,
        executable,
        has_icon,
        associations,
        listing,
        permissions,
    } = *app;
    associations.validate()?;
    let version = semver::Version::parse(version).map_err(|error| error.to_string())?;
    let version = format!("{}.{}.{}", version.major, version.minor, version.patch);
    let icon = if has_icon {
        "<key>CFBundleIconFile</key><string>app.icns</string>"
    } else {
        ""
    };
    let category = listing
        .apple_category()
        .map(|category| {
            format!(
                "<key>LSApplicationCategoryType</key><string>public.app-category.{category}</string>"
            )
        })
        .unwrap_or_default();
    let mut types = String::new();
    let schemes = associations
        .schemes()
        .map(|scheme| format!("<string>{}</string>", xml(scheme)))
        .collect::<String>();
    if !schemes.is_empty() {
        types.push_str(&format!("<key>CFBundleURLTypes</key><array><dict><key>CFBundleURLName</key><string>{}</string><key>CFBundleTypeRole</key><string>Editor</string><key>CFBundleURLSchemes</key><array>{schemes}</array></dict></array>\n", xml(id)));
    }
    types.push_str(&crate::desktop::uti::document_types(
        id,
        associations.documents(),
    ));
    let usage = usage_descriptions(name, permissions);
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{}</string>
<key>CFBundleName</key><string>{}</string>
<key>CFBundleDisplayName</key><string>{}</string>
<key>CFBundleExecutable</key><string>{}</string>
<key>CFBundleVersion</key><string>{version}</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
{icon}
{category}
{types}
{usage}
</dict></plist>
"#,
        xml(id),
        xml(name),
        xml(name),
        xml(executable)
    ))
}

/// What macOS tells people the first time the app's pages use a camera, a
/// microphone or their location: the reason `[permissions]` gives, or a
/// plain one.
fn usage_descriptions(name: &str, permissions: &[DeclaredPermission]) -> String {
    let mut usage = String::new();
    for declared in permissions {
        let (keys, uses): (&[&str], &str) = match declared.permission {
            Permission::Camera => (&["NSCameraUsageDescription"], "your camera"),
            Permission::Microphone => (&["NSMicrophoneUsageDescription"], "your microphone"),
            Permission::Geolocation => (
                &[
                    "NSLocationUsageDescription",
                    "NSLocationWhenInUseUsageDescription",
                ],
                "your location",
            ),
            _ => continue,
        };
        let reason = declared
            .reason
            .clone()
            .unwrap_or_else(|| format!("{name} uses {uses}."));
        for key in keys {
            usage.push_str(&format!(
                "<key>{key}</key><string>{}</string>\n",
                xml(&reason)
            ));
        }
    }
    usage
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_macos_the_reasons_pages_use_devices() {
        let usage = usage_descriptions(
            "Notes",
            &[
                DeclaredPermission {
                    permission: Permission::Camera,
                    reason: Some("Scan receipts & notes".to_string()),
                },
                DeclaredPermission {
                    permission: Permission::Microphone,
                    reason: None,
                },
                DeclaredPermission {
                    permission: Permission::Midi,
                    reason: None,
                },
            ],
        );
        assert_eq!(
            usage,
            "<key>NSCameraUsageDescription</key><string>Scan receipts &amp; notes</string>\n<key>NSMicrophoneUsageDescription</key><string>Notes uses your microphone.</string>\n"
        );
    }
}
