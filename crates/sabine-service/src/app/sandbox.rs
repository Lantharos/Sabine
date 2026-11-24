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
const OBJECT: char = '@';
/// What `windows` takes: recording the windows the app picks, such as a
/// game, and showing overlays over them.
const CAPTURE: &str = "capture";

/// Names an app would use to widen its own sandbox: the permission store
/// and portal backends, Flatpak's command runners, systemd and the bus itself.
const REFUSED_NAMES: [&str; 5] = [
    "org.freedesktop.impl.portal.*",
    "org.freedesktop.portal.Flatpak",
    "org.freedesktop.Flatpak",
    "org.freedesktop.systemd1",
    "org.freedesktop.DBus",
];

/// Name spaces of the desktop's own services, which an app that owned a name
/// in them could stand in for: the portals, notifications, the Secret
/// Service, the tray's watcher, the shell, accessibility and the like.
const DESKTOP_NAMESPACES: [&str; 6] = [
    "org.freedesktop.*",
    "org.gnome.*",
    "org.kde.*",
    "com.canonical.*",
    "org.ayatana.*",
    "org.a11y.*",
];
/// Where every media player's name sits, which a wildcard over it would let
/// the app take from players that aren't running.
const PLAYERS: &str = "org.mpris.MediaPlayer2";

/// `[sandbox]` as written in `Sabine.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxSection {
    network: Option<bool>,
    filesystems: Option<Vec<String>>,
    devices: Option<Vec<String>>,
    talk: Option<Vec<String>>,
    own: Option<Vec<String>>,
    pipewire: Option<bool>,
    x11: Option<bool>,
    microphone: Option<bool>,
    camera: Option<bool>,
    windows: Option<String>,
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
        own: Vec<String>,
        pipewire: bool,
        x11: bool,
        microphone: bool,
        camera: bool,
        /// Records the windows it picks and shows overlays over them, which
        /// Kestrel lets it do without asking.
        record_windows: bool,
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
                || self.own.is_some()
                || self.pipewire.is_some()
                || self.x11.is_some()
                || self.microphone.is_some()
                || self.camera.is_some()
                || self.windows.is_some()
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
        let own = self.own.unwrap_or_default();
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
        for entry in &talk {
            validate_talk(entry)?;
        }
        unique("talk", talk.iter().map(String::as_str))?;
        for name in &own {
            validate_owned_name(name)?;
        }
        unique("own", own.iter().map(String::as_str))?;
        if let Some(windows) = self
            .windows
            .as_deref()
            .filter(|windows| *windows != CAPTURE)
        {
            return Err(format!(
                "[sandbox] windows has `{windows}`; use `{CAPTURE}`"
            ));
        }
        Ok(Sandbox::Confined {
            network: self.network.unwrap_or(true),
            filesystems,
            devices,
            talk,
            own,
            pipewire: self.pipewire.unwrap_or(false),
            x11: self.x11.unwrap_or(false),
            microphone: self.microphone.unwrap_or(false),
            camera: self.camera.unwrap_or(false),
            record_windows: self.windows.is_some(),
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
                own,
                pipewire,
                x11,
                microphone,
                camera,
                record_windows,
            } => {
                let mut table = format!("\n[sandbox]\nnetwork = {network}\n");
                for (key, values) in [
                    ("filesystems", filesystems),
                    ("devices", devices),
                    ("talk", talk),
                    ("own", own),
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
                for (key, wanted) in [
                    ("pipewire", pipewire),
                    ("x11", x11),
                    ("microphone", microphone),
                    ("camera", camera),
                ] {
                    if *wanted {
                        table.push_str(&format!("{key} = true\n"));
                    }
                }
                if *record_windows {
                    table.push_str(&format!("windows = \"{CAPTURE}\"\n"));
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

/// The bus name a `talk` entry reaches, and the one object on it when the
/// entry is scoped to an object, as `name@/object/path`.
pub fn talk_target(entry: &str) -> (&str, Option<&str>) {
    match entry.split_once(OBJECT) {
        Some((name, path)) => (name, Some(path)),
        None => (entry, None),
    }
}

fn validate_talk(entry: &str) -> Result<(), String> {
    let (name, path) = talk_target(entry);
    validate_bus_name(name)?;
    let Some(path) = path else {
        return Ok(());
    };
    if name.ends_with(WILDCARD) {
        return Err(format!(
            "[sandbox] talk has `{entry}`; an object belongs to one name, so name it without `{WILDCARD}`"
        ));
    }
    if !valid_object_path(path) {
        return Err(format!(
            "[sandbox] talk has `{entry}`, whose `{path}` isn't a D-Bus object path"
        ));
    }
    Ok(())
}

fn valid_object_path(path: &str) -> bool {
    path == "/"
        || path.strip_prefix('/').is_some_and(|elements| {
            elements.split('/').all(|element| {
                !element.is_empty()
                    && element
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            })
        })
}

fn validate_owned_name(name: &str) -> Result<(), String> {
    if name.contains(OBJECT) {
        return Err(format!(
            "[sandbox] own has `{name}`; an app owns whole names, without an object"
        ));
    }
    validate_name("own", name)?;
    if let Some(service) = DESKTOP_NAMESPACES
        .iter()
        .find(|service| names_overlap(name, service))
    {
        return Err(format!(
            "[sandbox] own can't name `{name}`: it reaches `{service}`, where the app could stand in for the desktop's own services"
        ));
    }
    if name
        .strip_suffix(WILDCARD)
        .is_some_and(|base| base == PLAYERS || PLAYERS.starts_with(&format!("{base}.")))
    {
        return Err(format!(
            "[sandbox] own can't name `{name}`, which covers every media player; name the app's own player, such as `{PLAYERS}.notes`"
        ));
    }
    Ok(())
}

fn validate_bus_name(name: &str) -> Result<(), String> {
    validate_name("talk", name)
}

fn validate_name(key: &str, name: &str) -> Result<(), String> {
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
            "[sandbox] {key} has `{name}`, which isn't a D-Bus name"
        ));
    }
    if let Some(refused) = REFUSED_NAMES
        .iter()
        .find(|refused| names_overlap(name, refused))
    {
        return Err(format!(
            "[sandbox] {key} can't name `{name}`: it reaches `{refused}`, which would let the app leave its sandbox. Declare `unsandboxed = true` instead"
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
    fn the_display_and_recording_it_asks_for_reach_the_manifest() {
        assert_eq!(
            parse("network = false\nx11 = true\nmicrophone = true\ncamera = false")
                .unwrap()
                .manifest(),
            "\n[sandbox]\nnetwork = false\nx11 = true\nmicrophone = true\n"
        );
        assert!(parse("unsandboxed = true\nreason = \"x\"\ncamera = true").is_err());
    }

    #[test]
    fn recording_windows_reaches_the_manifest() {
        assert_eq!(
            parse("windows = \"capture\"").unwrap().manifest(),
            "\n[sandbox]\nnetwork = true\nwindows = \"capture\"\n"
        );
        assert!(parse("windows = \"control\"").is_err());
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

    #[test]
    fn owns_its_own_names_only() {
        let sandbox = parse("own = [\"org.mpris.MediaPlayer2.notes\", \"org.mpris.MediaPlayer2.notes.*\"]\npipewire = true").unwrap();
        assert_eq!(
            sandbox.manifest(),
            "\n[sandbox]\nnetwork = true\nown = [\"org.mpris.MediaPlayer2.notes\", \"org.mpris.MediaPlayer2.notes.*\"]\npipewire = true\n"
        );
        for refused in [
            "org.freedesktop.Notifications",
            "org.freedesktop.portal.Desktop",
            "org.kde.StatusNotifierWatcher",
            "org.gnome.Mutter.IdleMonitor",
            "org.*",
            "org.mpris.MediaPlayer2.*",
            "org.mpris.*",
            "com.example.Shell@/com/example",
            "org.freedesktop.systemd1",
        ] {
            assert!(
                parse(&format!("own = [\"{refused}\"]")).is_err(),
                "{refused}"
            );
        }
        assert!(parse("unsandboxed = true\nreason = \"x\"\npipewire = true").is_err());
    }

    #[test]
    fn talk_scoped_to_one_object() {
        assert!(parse("talk = [\"com.example.Shell@/com/example/Shell/Appearance\"]").is_ok());
        assert!(parse("talk = [\"com.example.Shell@/\"]").is_ok());
        assert!(parse("talk = [\"com.example.Shell@com/example\"]").is_err());
        assert!(parse("talk = [\"com.example.Shell@/com/example/\"]").is_err());
        assert!(parse("talk = [\"com.example.Shell@/com//example\"]").is_err());
        assert!(parse("talk = [\"com.example.Shell@/com/ex-ample\"]").is_err());
        assert!(parse("talk = [\"com.example.*@/com/example\"]").is_err());
        assert!(parse("talk = [\"org.freedesktop.systemd1@/org/freedesktop/systemd1\"]").is_err());
    }
}
