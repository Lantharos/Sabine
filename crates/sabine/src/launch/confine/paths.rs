use std::path::{Path, PathBuf};

use landlock::{ABI, Access, AccessFs, BitFlags};

pub(super) const LANDLOCK: ABI = ABI::V9;

const SYSTEM: [&str; 15] = [
    "/usr",
    "/etc",
    "/run/host/etc",
    "/opt",
    "/lib",
    "/lib64",
    "/bin",
    "/sbin",
    "/var/lib",
    "/var/cache",
    "/var/opt",
    "/var/usrlocal",
    "/sys",
    "/nix/store",
    "/gnu/store",
];
const RUN: &str = "/run";
/// What of `/run` stays out of reach: the runtime folders of the person and
/// everyone else, the removable drives mounted for them, and the host's
/// files a container shows, apart from the host's settings.
const PRIVATE_RUN: [&str; 3] = ["user", "media", "host"];
/// The system's sockets that answer the lookups every program makes, of host
/// names and user accounts.
const LOOKUP_SOCKETS: [&str; 3] = [
    "/run/systemd/resolve/io.systemd.Resolve",
    "/run/systemd/userdb",
    "/run/nscd/socket",
];
const WAYLAND_DISPLAY: &str = "WAYLAND_DISPLAY";
const DEFAULT_WAYLAND_DISPLAY: &str = "wayland-0";
const SOUND_SOCKET: &str = "pulse/native";
const MEDIA_SOCKET: &str = "pipewire-0";
const DOCUMENTS: &str = "doc";
const CAMERA_PREFIX: &str = "video";
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

/// What an app reaches of the person's runtime folder besides the display,
/// sound and the documents handed to it.
pub(super) struct Runtime {
    /// The app's own folder, where its windows, browser and session bus
    /// keep their sockets.
    pub(super) own: PathBuf,
    /// The socket later launches of a single-instance app reach it through.
    pub(super) instance: Option<PathBuf>,
    /// Whether the app records sound, so it reaches the person's PulseAudio
    /// socket instead of its own server that only plays.
    pub(super) records: bool,
    /// Whether the app reaches PipeWire: it speaks to it itself, records
    /// sound or screens, or uses cameras.
    pub(super) media: bool,
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

    /// The system's programs, libraries and settings, the sockets of its
    /// lookups, and scratch space.
    pub(super) fn system(&mut self) -> &mut Self {
        for path in SYSTEM {
            self.allow(path, read());
        }
        for entry in std::fs::read_dir(RUN).into_iter().flatten().flatten() {
            if !PRIVATE_RUN.iter().any(|name| entry.file_name() == *name) {
                self.allow(entry.path(), read());
            }
        }
        for socket in LOOKUP_SOCKETS {
            self.allow(socket, AccessFs::ResolveUnix.into());
        }
        for path in SCRATCH {
            self.allow(path, write());
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
            .allow(&data, read());
        for folder in [data.join("logs"), own.profile.clone()] {
            if std::fs::create_dir_all(&folder).is_ok() {
                self.allow(folder, write());
            }
        }
        self
    }

    /// The desktop's display, PulseAudio when the app records sound, PipeWire
    /// when it speaks to it, records sound or screens or uses cameras, the
    /// documents the portal hands the app and its own folder: none of the
    /// other sockets in the person's runtime folder, such as the session bus,
    /// the service manager's or other apps'.
    pub(super) fn runtime(&mut self, runtime: &Runtime) -> &mut Self {
        if let Some(folder) = dirs::runtime_dir() {
            let display = std::env::var_os(WAYLAND_DISPLAY)
                .filter(|display| !display.is_empty())
                .unwrap_or_else(|| DEFAULT_WAYLAND_DISPLAY.into());
            self.allow(folder.join(display), AccessFs::ResolveUnix.into())
                .allow(folder.join(DOCUMENTS), write());
            if runtime.records {
                self.allow(folder.join(SOUND_SOCKET), AccessFs::ResolveUnix.into());
            }
            if runtime.media {
                self.allow(folder.join(MEDIA_SOCKET), AccessFs::ResolveUnix.into());
            }
        }
        self.allow(&runtime.own, write());
        if let Some(instance) = &runtime.instance {
            self.allow(instance, AccessFs::ResolveUnix.into());
        }
        self
    }

    pub(super) fn cameras(&mut self) -> &mut Self {
        for device in named(&[CAMERA_PREFIX]) {
            self.allow(device, write());
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
