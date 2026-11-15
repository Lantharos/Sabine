#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "linux")]
use wayland as platform;
#[cfg(target_os = "windows")]
use windows as platform;

pub use platform::DisplayWatcher;

use serde::{Deserialize, Serialize};
use winit::event_loop::ActiveEventLoop;

/// A rectangle of the desktop in physical pixels, in the coordinates windows
/// are positioned in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A display connected to the computer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    /// Tells displays apart while they stay connected.
    pub id: u64,
    pub name: Option<String>,
    pub bounds: DisplayRect,
    /// The part of the display windows open in, without the taskbar, dock or
    /// menu bar. Wayland doesn't tell apps.
    pub work_area: Option<DisplayRect>,
    pub scale_factor: f64,
    /// In hertz, where the desktop tells.
    pub refresh_rate: Option<f64>,
    /// The display the desktop puts new windows and its taskbar or menu bar
    /// on. Wayland has none.
    pub primary: bool,
}

/// The displays connected to the computer, as the window's event loop sees
/// them.
pub fn displays(event_loop: &dyn ActiveEventLoop) -> Vec<Display> {
    let primary = event_loop.primary_monitor();
    event_loop
        .available_monitors()
        .filter_map(|monitor| {
            let (bounds, work_area) = platform::rects(&monitor)?;
            Some(Display {
                id: monitor.native_id(),
                name: monitor.name().map(std::borrow::Cow::into_owned),
                bounds,
                work_area,
                scale_factor: monitor.scale_factor(),
                refresh_rate: monitor
                    .current_video_mode()
                    .and_then(|mode| mode.refresh_rate_millihertz())
                    .map(|rate| f64::from(rate.get()) / 1000.0),
                primary: primary.as_ref() == Some(&monitor),
            })
        })
        .collect()
}
