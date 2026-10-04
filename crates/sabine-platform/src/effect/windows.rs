// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Mica uses DWM attributes; blur/acrylic use the dynamically loaded
// SetWindowCompositionAttribute entry point and its accent-policy ABI.
// These are different contracts, not interchangeable names for one effect.

use std::sync::{Arc, OnceLock};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{
        DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_TABBEDWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
        DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
    },
    System::LibraryLoader::{GetProcAddress, LoadLibraryA},
};
use windows::core::s;
use winit::window::{Theme, Window};

use crate::{WindowBackgroundEffect, WindowOptions, WindowRegionRect};

const COMPOSITION_ACCENT_POLICY: u32 = 0x13;
const ACCENT_BLUR: u32 = 3;
const ACCENT_ACRYLIC: u32 = 4;

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    color: u32,
    animation_id: u32,
}

#[repr(C)]
struct CompositionAttributeData {
    attribute: u32,
    value: *mut std::ffi::c_void,
    size: usize,
}

type SetWindowCompositionAttribute =
    unsafe extern "system" fn(HWND, *mut CompositionAttributeData) -> i32;

pub(super) struct Backend {
    hwnd: HWND,
    effect: WindowBackgroundEffect,
}

impl Backend {
    pub(super) fn new(window: &Arc<dyn Window>, options: &WindowOptions) -> Option<Self> {
        let backend = Self {
            hwnd: hwnd(window)?,
            effect: if options.wants_background_effect() {
                options.background_effect
            } else {
                WindowBackgroundEffect::None
            },
        };
        backend.apply(window.theme().unwrap_or(Theme::Light));
        Some(backend)
    }

    pub(super) fn update(
        &mut self,
        _options: &WindowOptions,
        _width: i32,
        _height: i32,
        _transparent_holes: &[WindowRegionRect],
    ) {
    }

    pub(super) fn theme_changed(&mut self, theme: Theme) {
        self.apply(theme);
    }

    fn apply(&self, theme: Theme) {
        let dark = theme == Theme::Dark;
        let applied = match self.effect {
            WindowBackgroundEffect::None => return,
            WindowBackgroundEffect::Blur => set_accent(self.hwnd, ACCENT_BLUR, 0, dark),
            WindowBackgroundEffect::Mica => set_backdrop(self.hwnd, DWMSBT_MAINWINDOW),
            WindowBackgroundEffect::MicaAlt => set_backdrop(self.hwnd, DWMSBT_TABBEDWINDOW),
            _ => set_accent(self.hwnd, ACCENT_ACRYLIC, 125, dark),
        };
        set_dark_mode(self.hwnd, dark);
        if !applied && std::env::var_os("SABINE_TRACE").is_some() {
            eprintln!("Sabine window effect {:?} was not applied", self.effect);
        }
    }
}

fn set_dark_mode(hwnd: HWND, dark: bool) {
    let value = u32::from(dark);
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&value as *const u32).cast(),
            std::mem::size_of_val(&value) as u32,
        );
    }
}

fn set_backdrop(hwnd: HWND, backdrop: DWM_SYSTEMBACKDROP_TYPE) -> bool {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            (&backdrop as *const DWM_SYSTEMBACKDROP_TYPE).cast(),
            std::mem::size_of_val(&backdrop) as u32,
        )
    }
    .is_ok()
}

fn set_accent(hwnd: HWND, state: u32, opacity: u8, dark: bool) -> bool {
    let Some(set_window_composition_attribute) = set_window_composition_attribute() else {
        return false;
    };
    let channel = if dark { 18_u32 } else { 243_u32 };
    let mut policy = AccentPolicy {
        state,
        flags: u32::from(state == ACCENT_BLUR) * 2,
        color: channel | (channel << 8) | (channel << 16) | (u32::from(opacity) << 24),
        animation_id: 0,
    };
    let mut data = CompositionAttributeData {
        attribute: COMPOSITION_ACCENT_POLICY,
        value: (&mut policy as *mut AccentPolicy).cast(),
        size: std::mem::size_of::<AccentPolicy>(),
    };
    (unsafe { set_window_composition_attribute(hwnd, &mut data) }) != 0
}

fn set_window_composition_attribute() -> Option<SetWindowCompositionAttribute> {
    static FUNCTION: OnceLock<Option<SetWindowCompositionAttribute>> = OnceLock::new();
    *FUNCTION.get_or_init(|| unsafe {
        let user32 = LoadLibraryA(s!("user32.dll")).ok()?;
        let function = GetProcAddress(user32, s!("SetWindowCompositionAttribute"))?;
        Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            SetWindowCompositionAttribute,
        >(function))
    })
}

fn hwnd(window: &Arc<dyn Window>) -> Option<HWND> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    Some(HWND(handle.hwnd.get() as *mut std::ffi::c_void))
}
