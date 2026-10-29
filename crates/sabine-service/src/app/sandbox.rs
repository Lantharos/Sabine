use serde::Deserialize;

const PLACES: [&str; 7] = [
    "home",
    "xdg-desktop",
    "xdg-documents",
    "xdg-download",
    "xdg-music",
    "xdg-pictures",
    "xdg-videos",
];
const READ_ONLY: &str = ":ro";
const DEVICES: [&str; 3] = ["usb", "serial", "all"];
const WILDCARD: &str = ".*";
const MAX_BUS_NAME: usize = 255;

/// Names an app would use to widen its own sandbox: the permission store
/// and portal backends, Flatpak's command runners, systemd and the bus itself.
const REFUSED_NAMES: [&str; 5] = [
    "org.freedesktop.impl.portal.*",
    "org.freedesktop.portal.Flatpak",
    "org.freedesktop.Flatpak",
    "org.freedesktop.systemd1",
    "org.freedesktop.DBus",
];

/// `[sandbox]` as written in `Sabine.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxSection {
    network: Option<bool>,
    filesystems: Option<Vec<String>>,
    devices: Option<Vec<String>>,
    talk: Option<Vec<String>>,
    #[serde(default)]
    unsandboxed: bool,
    reason: Option<String>,
}

/// What the app declares it needs from a sandbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sandbox {
    Confined {
        network: bool,
        filesystems: Vec<String>,
        devices: Vec<String>,
        talk: Vec<String>,
    },
    Unsandboxed {
        reason: String,
    },
}

impl SandboxSection {
    pub fn resolve(self) -> Result<Sandbox, String> {
        if self.unsandboxed {
            if self.network.is_some()
                || self.filesystems.is_some()
                || self.devices.is_some()
                || self.talk.is_some()
            {
                return Err(
                    "[sandbox] of an unsandboxed app takes only `unsandboxed` and `reason`"
                        .to_string(),
                );
            }
            let reason = self.reason.unwrap_or_default().trim().to_string();
            if reason.is_empty() || reason.chars().any(char::is_control) {
                return Err(
                    "[sandbox] reason must say in one line why the app can't run in a sandbox"
                        .to_string(),
                );
            }
            return Ok(Sandbox::Unsandboxed { reason });
        }
        if self.reason.is_some() {
            return Err("[sandbox] reason is only for apps with `unsandboxed = true`".to_string());
        }
        let filesystems = self.filesystems.unwrap_or_default();
        let devices = self.devices.unwrap_or_default();
        let talk = self.talk.unwrap_or_default();
        for entry in &filesystems {
            let place = entry.strip_suffix(READ_ONLY).unwrap_or(entry);
            if !PLACES.contains(&place) {
                return Err(format!(
                    "[sandbox] filesystems has `{entry}`; use {}, optionally ending in `{READ_ONLY}`",
                    PLACES.join(", ")
                ));
            }
        }
        unique(
            "filesystems",
            filesystems
                .iter()
                .map(|entry| entry.strip_suffix(READ_ONLY).unwrap_or(entry)),
        )?;
        for device in &devices {
            if !DEVICES.contains(&device.as_str()) {
                return Err(format!(
                    "[sandbox] devices has `{device}`; use {}",
                    DEVICES.join(", ")
                ));
            }
        }
        unique("devices", devices.iter().map(String::as_str))?;
        for name in &talk {
            validate_bus_name(name)?;
        }
        unique("talk", talk.iter().map(String::as_str))?;
        Ok(Sandbox::Confined {
            network: self.network.unwrap_or(true),
            filesystems,
            devices,
            talk,
        })
    }
}

impl Sandbox {
    /// The normalized `[sandbox]` table of the runtime manifest.
    pub fn manifest(&self) -> String {
        match self {
            Self::Unsandboxed { reason } => format!(
                "\n[sandbox]\nunsandboxed = true\nreason = {}\n",
                toml_string(reason)
            ),
            Self::Confined {
                network,
                filesystems,
                devices,
                talk,
            } => {
                let mut table = format!("\n[sandbox]\nnetwork = {network}\n");
                for (key, values) in [
                    ("filesystems", filesystems),
                    ("devices", devices),
                    ("talk", talk),
                ] {
                    if !values.is_empty() {
                        let list = values
                            .iter()
                            .map(|value| toml_string(value))
                            .collect::<Vec<_>>()
                            .join(", ");
                        table.push_str(&format!("{key} = [{list}]\n"));
                    }
                }
                table
            }
        }
    }
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization")
}

fn unique<'a>(key: &str, values: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut seen = Vec::new();
    for value in values {
        if seen.contains(&value) {
            return Err(format!("[sandbox] {key} names `{value}` twice"));
        }
        seen.push(value);
    }
    Ok(())
}

fn validate_bus_name(name: &str) -> Result<(), String> {
    let base = name.strip_suffix(WILDCARD).unwrap_or(name);
    let elements: Vec<&str> = base.split('.').collect();
    let valid = name.len() <= MAX_BUS_NAME
        && elements.len() >= 2
        && elements.iter().all(|element| {
            !element.is_empty()
                && !element.starts_with(|ch: char| ch.is_ascii_digit())
                && element
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        });
    if !valid {
        return Err(format!(
            "[sandbox] talk has `{name}`, which isn't a D-Bus name"
        ));
    }
    if let Some(refused) = REFUSED_NAMES
        .iter()
        .find(|refused| names_overlap(name, refused))
    {
        return Err(format!(
            "[sandbox] talk can't name `{name}`: it reaches `{refused}`, which would let the app leave its sandbox. Declare `unsandboxed = true` instead"
        ));
    }
    Ok(())
}

fn names_overlap(first: &str, second: &str) -> bool {
    let parts = |name: &str| match name.strip_suffix(WILDCARD) {
        Some(base) => (base.to_string(), true),
        None => (name.to_string(), false),
    };
    let (first, first_wild) = parts(first);
    let (second, second_wild) = parts(second);
    let covers = |wide: &str, narrow: &str| narrow.starts_with(&format!("{wide}."));
    first == second
        || first_wild && covers(&first, &second)
        || second_wild && covers(&second, &first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Sandbox, String> {
        toml::from_str::<SandboxSection>(text)
            .map_err(|error| error.to_string())?
            .resolve()
    }

    #[test]
    fn confined_declaration_is_normalized() {
        let sandbox = parse(
            "filesystems = [\"xdg-documents\", \"xdg-download:ro\"]\ndevices = [\"usb\"]\ntalk = [\"org.mpris.MediaPlayer2.notes\"]",
        )
        .unwrap();
        assert_eq!(
            sandbox.manifest(),
            "\n[sandbox]\nnetwork = true\nfilesystems = [\"xdg-documents\", \"xdg-download:ro\"]\ndevices = [\"usb\"]\ntalk = [\"org.mpris.MediaPlayer2.notes\"]\n"
        );
    }

    #[test]
    fn unsandboxed_needs_a_reason_and_nothing_else() {
        assert!(parse("unsandboxed = true").is_err());
        assert!(parse("unsandboxed = true\nreason = \"x\"\nnetwork = false").is_err());
        assert_eq!(
            parse("unsandboxed = true\nreason = \"Runs your containers\"").unwrap(),
            Sandbox::Unsandboxed {
                reason: "Runs your containers".to_string()
            }
        );
        assert!(parse("reason = \"x\"").is_err());
    }

    #[test]
    fn rejects_unknown_places_devices_and_escaping_names() {
        assert!(parse("filesystems = [\"host\"]").is_err());
        assert!(parse("filesystems = [\"xdg-music\", \"xdg-music:ro\"]").is_err());
        assert!(parse("devices = [\"camera\"]").is_err());
        assert!(parse("talk = [\"org.freedesktop.impl.portal.PermissionStore\"]").is_err());
        assert!(parse("talk = [\"org.freedesktop.*\"]").is_err());
        assert!(parse("talk = [\"org.freedesktop.Flatpak\"]").is_err());
        assert!(parse("talk = [\"notdbus\"]").is_err());
        assert!(parse("talk = [\"org.kde.StatusNotifierItem-1.*\"]").is_ok());
    }
}
