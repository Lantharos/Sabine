use winit::event_loop::EventLoopProxy;
use winit::window::Window;

#[derive(Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) background: u32,
    pub(crate) panel: u32,
    pub(crate) track: u32,
    pub(crate) text: u32,
    pub(crate) muted: u32,
    pub(crate) button: u32,
    pub(crate) button_hovered: u32,
    pub(crate) button_pressed: u32,
    pub(crate) primary: u32,
    pub(crate) primary_hovered: u32,
    pub(crate) primary_pressed: u32,
}

const DARK: Palette = Palette {
    background: 0xFF_16_16_18,
    panel: 0xFF_20_20_23,
    track: 0xFF_2A_2A_2E,
    text: 0xFF_F4_F4_F5,
    muted: 0xFF_A1_A1_AA,
    button: 0xFF_2A_2A_2E,
    button_hovered: 0xFF_3A_3A_40,
    button_pressed: 0xFF_46_46_4B,
    primary: 0xFF_E8_E8_EA,
    primary_hovered: 0xFF_FF_FF_FF,
    primary_pressed: 0xFF_BB_BB_BF,
};

const LIGHT: Palette = Palette {
    background: 0xFF_FA_FA_FA,
    panel: 0xFF_EF_EF_F1,
    track: 0xFF_E4_E4_E7,
    text: 0xFF_18_18_1B,
    muted: 0xFF_6B_6B_74,
    button: 0xFF_E8_E8_EB,
    button_hovered: 0xFF_DD_DD_E1,
    button_pressed: 0xFF_D1_D1_D6,
    primary: 0xFF_1F_1F_23,
    primary_hovered: 0xFF_3A_3A_41,
    primary_pressed: 0xFF_52_52_5A,
};

/// Follows the desktop's light or dark style, waking the window's event loop
/// when it changes so the window can paint itself again.
pub(crate) struct Theme {
    #[cfg(target_os = "linux")]
    watcher: sabine_platform::AppearanceWatcher,
}

impl Theme {
    #[cfg(target_os = "linux")]
    pub(crate) fn follow(proxy: EventLoopProxy) -> Self {
        Self {
            watcher: sabine_platform::AppearanceWatcher::start(move || proxy.wake_up()),
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn follow(_proxy: EventLoopProxy) -> Self {
        Self {}
    }

    pub(crate) fn palette(&self, window: &dyn Window) -> Palette {
        #[cfg(target_os = "linux")]
        let dark = self.watcher.appearance(window).dark;
        #[cfg(not(target_os = "linux"))]
        let dark = sabine_platform::system_appearance(window).dark;
        if dark { DARK } else { LIGHT }
    }
}
