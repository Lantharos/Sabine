use winit::window::{Theme, Window};

use super::Appearance;

/// The window's theme with the system accent color, read again whenever the
/// window asks, since Windows and macOS report accent changes to no window.
pub fn system_appearance(window: &dyn Window) -> Appearance {
    Appearance {
        dark: window.theme() == Some(Theme::Dark),
        accent: accent_color(),
    }
}

#[cfg(target_os = "windows")]
fn accent_color() -> Option<[u8; 3]> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    use windows::core::w;

    let mut value = 0_u32;
    let mut size = size_of::<u32>() as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\DWM"),
            w!("AccentColor"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    }
    .is_ok()
    .then(|| {
        let [red, green, blue, _] = value.to_le_bytes();
        [red, green, blue]
    })
}

#[cfg(target_os = "macos")]
fn accent_color() -> Option<[u8; 3]> {
    use objc2_app_kit::{NSColor, NSColorSpace};

    let accent =
        NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some([
        super::channel(accent.redComponent()),
        super::channel(accent.greenComponent()),
        super::channel(accent.blueComponent()),
    ])
}
