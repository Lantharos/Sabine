#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BundleFormat {
    Linux,
    Portable,
    Deb,
    Rpm,
    AppImage,
    Windows,
    Exe,
    Msi,
    Macos,
    Dmg,
}

impl BundleFormat {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "linux" => Some(Self::Linux),
            "portable" | "tar" | "tar.gz" => Some(Self::Portable),
            "deb" => Some(Self::Deb),
            "rpm" => Some(Self::Rpm),
            "appimage" => Some(Self::AppImage),
            "windows" => Some(Self::Windows),
            "exe" | "nsis" => Some(Self::Exe),
            "msi" => Some(Self::Msi),
            "macos" | "app" => Some(Self::Macos),
            "dmg" => Some(Self::Dmg),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Portable => "portable",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::AppImage => "appimage",
            Self::Windows => "windows",
            Self::Exe => "exe",
            Self::Msi => "msi",
            Self::Macos => "macos",
            Self::Dmg => "dmg",
        }
    }

    /// The operating system the bundle runs on; a portable bundle is for
    /// the one building it.
    pub(crate) fn target(self) -> BuildTarget {
        match self {
            Self::Linux | Self::Deb | Self::Rpm | Self::AppImage => BuildTarget::Linux,
            Self::Windows | Self::Exe | Self::Msi => BuildTarget::Windows,
            Self::Macos | Self::Dmg => BuildTarget::Macos,
            Self::Portable => BuildTarget::host(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuildTarget {
    Linux,
    Windows,
    Macos,
}

impl BuildTarget {
    fn host() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Linux
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::Macos => "macos",
        }
    }

    pub(crate) fn rust_target(self) -> Option<&'static str> {
        match self {
            Self::Linux if cfg!(target_os = "linux") => None,
            Self::Linux if cfg!(target_arch = "aarch64") => Some("aarch64-unknown-linux-gnu"),
            Self::Linux => Some("x86_64-unknown-linux-gnu"),
            Self::Windows if cfg!(target_arch = "aarch64") => Some("aarch64-pc-windows-msvc"),
            Self::Windows => Some("x86_64-pc-windows-msvc"),
            Self::Macos => Some("aarch64-apple-darwin"),
        }
    }
}
