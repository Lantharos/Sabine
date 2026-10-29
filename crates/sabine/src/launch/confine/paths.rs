use std::path::{Path, PathBuf};

use landlock::{ABI, Access, AccessFs, BitFlags};

pub(super) const LANDLOCK: ABI = ABI::V9;

const SYSTEM: [&str; 11] = [
    "/usr",
    "/etc",
    "/opt",
    "/lib",
    "/lib64",
    "/bin",
    "/sbin",
    "/var",
    "/sys",
    "/nix/store",
    "/gnu/store",
];
const SOCKETS: &str = "/run";
const SCRATCH: [&str; 3] = ["/tmp", "/var/tmp", "/proc"];
const DEVICES: &str = "/dev";
const COMMON_DEVICES: [&str; 10] = [
    "null", "zero", "full", "random", "urandom", "tty", "ptmx", "pts", "shm", "dri",
];
const GPU_PREFIX: &str = "nvidia";
const USB: &str = "bus/usb";
const USB_PREFIXES: [&str; 1] = ["hidraw"];
const SERIAL_PREFIXES: [&str; 2] = ["ttyUSB", "ttyACM"];
const READ_ONLY: &str = ":ro";
const HOME_SETTINGS: [&str; 3] = [".fonts", ".icons", ".themes"];
const CONFIG_SETTINGS: [&str; 5] = [
    "fontconfig",
    "gtk-3.0",
    "gtk-4.0",
    "mimeapps.list",
    "user-dirs.dirs",
];
const DATA_SETTINGS: [&str; 4] = ["fonts", "icons", "themes", "mime"];
const SHARED_CACHES: [&str; 5] = [
    "fontconfig",
    "mesa_shader_cache",
    "mesa_shader_cache_db",
    "radv_builtin_shaders",
    "nvidia",
];

/// The folders and files an app reaches, each with what it may do there.
#[derive(Default)]
pub(super) struct Paths(pub(super) Vec<(PathBuf, BitFlags<AccessFs>)>);

/// What an app needs of its own: its program, its Chromium runtime and
/// Sabine's files it reads, and where it keeps its data.
pub(super) struct Own<'a> {
    pub(super) app_id: &'a str,
    pub(super) program: Option<PathBuf>,
    pub(super) web_root: Option<PathBuf>,
    pub(super) runtime: &'a Path,
    pub(super) host: &'a Path,
    pub(super) profile: PathBuf,
}

fn read() -> BitFlags<AccessFs> {
    AccessFs::from_read(LANDLOCK)
}

fn write() -> BitFlags<AccessFs> {
    AccessFs::from_all(LANDLOCK)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn named(prefixes: &[&str]) -> Vec<PathBuf> {
    std::fs::read_dir(DEVICES)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            prefixes
                .iter()
                .any(|prefix| name.to_string_lossy().starts_with(prefix))
        })
        .map(|entry| entry.path())
        .collect()
}

/// The place a `filesystems` entry of `[sandbox]` names, and whether it's
/// read-only.
fn declared(entry: &str) -> Option<(PathBuf, bool)> {
    let place = entry.strip_suffix(READ_ONLY);
    let folder = match place.unwrap_or(entry) {
        "home" => dirs::home_dir(),
        "xdg-desktop" => dirs::desktop_dir(),
        "xdg-documents" => dirs::document_dir(),
        "xdg-download" => dirs::download_dir(),
        "xdg-music" => dirs::audio_dir(),
        "xdg-pictures" => dirs::picture_dir(),
        "xdg-videos" => dirs::video_dir(),
        _ => None,
    }?;
    Some((folder, place.is_some()))
}

impl Paths {
    fn allow(&mut self, path: impl Into<PathBuf>, access: BitFlags<AccessFs>) -> &mut Self {
        self.0.push((path.into(), access));
        self
    }

    fn below(&mut self, folder: Option<PathBuf>, names: &[&str], access: BitFlags<AccessFs>) {
        let Some(folder) = folder else {
            return;
        };
        for name in names {
            self.allow(folder.join(name), access);
        }
    }

    /// The system's programs, libraries and settings, its sockets, and
    /// scratch space.
    pub(super) fn system(&mut self) -> &mut Self {
        for path in SYSTEM {
            self.allow(path, read());
        }
        self.allow(SOCKETS, read() | AccessFs::ResolveUnix);
        for path in SCRATCH {
            self.allow(path, write());
        }
        if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
            self.allow(runtime, write());
        }
        self.allow(DEVICES, AccessFs::ReadDir.into());
        for device in COMMON_DEVICES {
            self.allow(Path::new(DEVICES).join(device), write());
        }
        for device in named(&[GPU_PREFIX]) {
            self.allow(device, write());
        }
        self
    }

    /// The person's fonts, icons, themes and desktop settings, and the
    /// caches fonts and graphics drivers share between apps.
    pub(super) fn desktop(&mut self) -> &mut Self {
        self.below(home(), &HOME_SETTINGS, read());
        self.below(dirs::config_dir(), &CONFIG_SETTINGS, read());
        self.below(dirs::data_dir(), &DATA_SETTINGS, read());
        self.below(dirs::cache_dir(), &SHARED_CACHES, write());
        self
    }

    pub(super) fn own(&mut self, own: &Own<'_>) -> &mut Self {
        for folder in [
            dirs::data_dir(),
            dirs::config_dir(),
            dirs::cache_dir(),
            dirs::state_dir(),
        ]
        .into_iter()
        .flatten()
        {
            let folder = folder.join(own.app_id);
            if std::fs::create_dir_all(&folder).is_ok() {
                self.allow(folder, write());
            }
        }
        for folder in [&own.program, &own.web_root].into_iter().flatten() {
            self.allow(folder, read());
        }
        let data = sabine_runtime::sabine_data_dir();
        self.allow(own.runtime, read())
            .allow(own.runtime.join(".leases"), write())
            .allow(own.host, read())
            .allow(&data, read())
            .allow(data.join("logs"), write());
        if std::fs::create_dir_all(&own.profile).is_ok() {
            self.allow(&own.profile, write());
        }
        self
    }

    pub(super) fn declared(&mut self, filesystems: &[String], devices: &[String]) -> &mut Self {
        for (folder, read_only) in filesystems.iter().filter_map(|entry| declared(entry)) {
            self.allow(folder, if read_only { read() } else { write() });
        }
        for device in devices {
            match device.as_str() {
                "all" => {
                    self.allow(DEVICES, write());
                }
                "usb" => {
                    self.allow(Path::new(DEVICES).join(USB), write());
                    for device in named(&USB_PREFIXES) {
                        self.allow(device, write());
                    }
                }
                "serial" => {
                    for device in named(&SERIAL_PREFIXES) {
                        self.allow(device, write());
                    }
                }
                _ => {}
            }
        }
        self
    }
}
