// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Windows tells desktop apps about accent changes through the WinRT
// UISettings object, which raises ColorValuesChanged on a thread pool thread
// for as long as the object lives. The accent itself is read from DWM's
// registry value, as 0xAABBGGRR.

use std::sync::{Arc, Mutex};

use windows::Foundation::TypedEventHandler;
use windows::UI::ViewManagement::UISettings;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::w;
use winit::window::{Theme, Window};

use super::Appearance;

/// The window's theme with the system accent color, which it keeps current
/// and wakes the window about whenever Windows changes it.
pub struct AppearanceWatcher {
    accent: Arc<Mutex<Option<[u8; 3]>>>,
    settings: Option<(UISettings, i64)>,
}

impl AppearanceWatcher {
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let accent = Arc::new(Mutex::new(accent_color()));
        let watched = Arc::clone(&accent);
        let settings = UISettings::new().and_then(|settings| {
            let token = settings.ColorValuesChanged(&TypedEventHandler::new(move |_, _| {
                *watched.lock().unwrap_or_else(|error| error.into_inner()) = accent_color();
                wake();
                Ok(())
            }))?;
            Ok((settings, token))
        });
        Self {
            accent,
            settings: settings.ok(),
        }
    }

    pub fn appearance(&self, window: &dyn Window) -> Appearance {
        Appearance {
            dark: window.theme() == Some(Theme::Dark),
            accent: *self
                .accent
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        }
    }
}

impl Drop for AppearanceWatcher {
    fn drop(&mut self) {
        if let Some((settings, token)) = &self.settings {
            let _ = settings.RemoveColorValuesChanged(*token);
        }
    }
}

fn accent_color() -> Option<[u8; 3]> {
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
