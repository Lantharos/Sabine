use super::regions::WindowRegions;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowChrome {
    #[default]
    System,
    Sabine,
    Frameless,
}

impl WindowChrome {
    pub fn uses_native_decorations(self) -> bool {
        matches!(self, Self::System)
    }
}

/// The material a transparent window shows behind its page, where the page
/// leaves it see-through. Each desktop has materials of its own; `Glass` is
/// the standard one everywhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowBackgroundEffect {
    #[default]
    None,
    /// A blur of what is behind the window on Wayland compositors that offer
    /// `ext_background_effect_v1`, acrylic on Windows and the under-window
    /// material on macOS.
    Glass,
    /// A plain blur, without acrylic's tint.
    #[cfg(target_os = "windows")]
    Blur,
    /// The Windows 11 material tinted by the desktop background.
    #[cfg(target_os = "windows")]
    Mica,
    /// Mica tinted more strongly, as tabbed windows use it.
    #[cfg(target_os = "windows")]
    MicaAlt,
    /// The dark material of heads-up panels.
    #[cfg(target_os = "macos")]
    HudWindow,
    /// The material of sidebars such as Finder's.
    #[cfg(target_os = "macos")]
    Sidebar,
}

impl WindowBackgroundEffect {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "glass" => Some(Self::Glass),
            #[cfg(target_os = "windows")]
            "blur" => Some(Self::Blur),
            #[cfg(target_os = "windows")]
            "mica" => Some(Self::Mica),
            #[cfg(target_os = "windows")]
            "mica-alt" => Some(Self::MicaAlt),
            #[cfg(target_os = "macos")]
            "hud-window" => Some(Self::HudWindow),
            #[cfg(target_os = "macos")]
            "sidebar" => Some(Self::Sidebar),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Glass => "glass",
            #[cfg(target_os = "windows")]
            Self::Blur => "blur",
            #[cfg(target_os = "windows")]
            Self::Mica => "mica",
            #[cfg(target_os = "windows")]
            Self::MicaAlt => "mica-alt",
            #[cfg(target_os = "macos")]
            Self::HudWindow => "hud-window",
            #[cfg(target_os = "macos")]
            Self::Sidebar => "sidebar",
        }
    }

    pub fn requires_transparency(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowOptions {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub chrome: WindowChrome,
    pub resizable: bool,
    pub visible: bool,
    pub active: bool,
    pub always_on_top: bool,
    pub transparent: bool,
    pub background_effect: WindowBackgroundEffect,
    pub regions: WindowRegions,
}

impl WindowOptions {
    pub fn wants_background_effect(&self) -> bool {
        self.transparent && self.background_effect != WindowBackgroundEffect::None
    }
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "Sabine".to_string(),
            width: 760,
            height: 520,
            min_width: 420,
            min_height: 280,
            chrome: WindowChrome::System,
            resizable: true,
            visible: true,
            active: true,
            always_on_top: false,
            transparent: false,
            background_effect: WindowBackgroundEffect::None,
            regions: WindowRegions::default(),
        }
    }
}
