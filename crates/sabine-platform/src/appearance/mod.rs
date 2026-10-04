#[cfg(target_os = "linux")]
mod portal;
#[cfg(not(target_os = "linux"))]
mod system;

#[cfg(target_os = "linux")]
pub use portal::AppearanceWatcher;
#[cfg(not(target_os = "linux"))]
pub use system::system_appearance;

/// The desktop's light or dark preference and the accent color people chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appearance {
    pub dark: bool,
    /// The accent color as sRGB, where the desktop has one.
    pub accent: Option<[u8; 3]>,
}

impl Appearance {
    /// The accent color as a CSS hex color.
    pub fn accent_hex(&self) -> Option<String> {
        self.accent
            .map(|[red, green, blue]| format!("#{red:02x}{green:02x}{blue:02x}"))
    }
}

#[cfg(not(target_os = "windows"))]
fn channel(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
