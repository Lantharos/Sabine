//! Where a window opens: centered on the launch monitor, at the size it
//! last closed with.

pub(crate) mod remembered_size;

use winit::{dpi::PhysicalPosition, event_loop::ActiveEventLoop, monitor::MonitorHandle};

/// Where an overlay of `width` × `height` logical pixels goes on the launch
/// screen, at its placement.
#[cfg(not(target_os = "linux"))]
pub(crate) fn overlay_position(
    event_loop: &dyn ActiveEventLoop,
    overlay: crate::SabineOverlay,
    width: u32,
    height: u32,
) -> Option<PhysicalPosition<i32>> {
    let monitor = launch_monitor(event_loop)?;
    let screen = monitor.current_video_mode()?.size();
    let origin = monitor.position()?;
    let scale = monitor.scale_factor().max(1.0);
    let scaled = crate::SabineOverlay {
        margin: (f64::from(overlay.margin) * scale).round() as u32,
        ..overlay
    };
    let (x, y) = scaled.origin(
        (f64::from(origin.x), f64::from(origin.y)),
        (f64::from(screen.width), f64::from(screen.height)),
        (f64::from(width) * scale, f64::from(height) * scale),
    );
    Some(PhysicalPosition::new(x.round() as i32, y.round() as i32))
}

pub(crate) fn centered_window_position(
    event_loop: &dyn ActiveEventLoop,
    width: u32,
    height: u32,
) -> Option<PhysicalPosition<i32>> {
    let monitor = launch_monitor(event_loop)?;
    let mode = monitor.current_video_mode()?;
    let monitor_size = mode.size();
    let monitor_position = monitor.position()?;
    let scale = monitor.scale_factor().max(1.0);
    let physical_width = (f64::from(width) * scale).round() as i32;
    let physical_height = (f64::from(height) * scale).round() as i32;
    let x = monitor_position.x + (monitor_size.width as i32 - physical_width).max(0) / 2;
    let y = monitor_position.y + (monitor_size.height as i32 - physical_height).max(0) / 2;
    Some(PhysicalPosition::new(x, y))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn launch_scale_factor(event_loop: &dyn ActiveEventLoop) -> f64 {
    launch_monitor(event_loop).map_or(1.0, |monitor| monitor.scale_factor().max(1.0))
}

fn launch_monitor(event_loop: &dyn ActiveEventLoop) -> Option<MonitorHandle> {
    event_loop
        .primary_monitor()
        .or_else(|| event_loop.available_monitors().next())
}
