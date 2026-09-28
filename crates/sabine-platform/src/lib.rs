#[cfg(any(target_os = "windows", target_os = "macos"))]
#[path = "platform/desktop_background_effect.rs"]
mod desktop_background_effect;
mod desktop_integration;
mod regions;
#[cfg(not(target_os = "macos"))]
#[path = "platform/shortcut_inhibit/mod.rs"]
mod shortcut_inhibit;
#[path = "platform/wayland_background_effect.rs"]
mod wayland_background_effect;
#[cfg(target_os = "linux")]
#[path = "platform/wayland_client.rs"]
mod wayland_client;
mod window_options;

use std::sync::Arc;

use winit::window::Window;

pub use desktop_integration::{
    AutostartEntry, DeepLinkRegistration, GlobalShortcutActivation, GlobalShortcutRegistration,
    NativeMessagingHost, PlatformEvent, Shortcut, ShortcutModifiers, SingleInstanceActivation,
    SingleInstancePolicy, TrayActivation, TrayIcon, TrayMenuItem,
};
pub use regions::{WindowRegion, WindowRegionAdaptive, WindowRegionRect, WindowRegions};
#[cfg(not(target_os = "macos"))]
pub use shortcut_inhibit::ShortcutInhibitor;
#[cfg(target_os = "windows")]
pub use shortcut_inhibit::SystemKey;
pub use wayland_background_effect::WaylandEffect as WindowEffect;
pub use window_options::{
    PlatformOs, WindowBackgroundEffect, WindowChrome, WindowOptions, current_desktop_os,
};

pub fn request_window_effect(
    window: &Arc<dyn Window>,
    options: &WindowOptions,
) -> Option<WindowEffect> {
    #[cfg(target_os = "linux")]
    {
        wayland_background_effect::request(window, options)
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        desktop_background_effect::request(window, options)
    }
}
