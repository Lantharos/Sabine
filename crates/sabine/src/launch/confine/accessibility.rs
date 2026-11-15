//! The accessibility bus a confined app reaches, which screen readers read
//! apps through. It is filtered as Flatpak filters it, so the app offers its
//! own accessibility and announces its changes, but can't read or drive the
//! other apps on the desktop.

use std::{ffi::OsString, path::Path};

const ADDRESS: &str = "AT_SPI_BUS_ADDRESS";
const LAUNCHER: &str = "org.a11y.Bus";
const LAUNCHER_PATH: &str = "/org/a11y/bus";

/// What the app may ask of the session bus's accessibility service: whether
/// assistive technology is running, and to hear when that changes.
pub(super) const STATUS_RULES: [&str; 3] = [
    "--call=org.a11y.Bus=org.freedesktop.DBus.Properties.Get@/org/a11y/bus",
    "--call=org.a11y.Bus=org.freedesktop.DBus.Properties.GetAll@/org/a11y/bus",
    "--broadcast=org.a11y.Bus=org.freedesktop.DBus.Properties.PropertiesChanged@/org/a11y/bus",
];

const RULES: [&str; 9] = [
    "--broadcast=org.a11y.atspi.Registry=org.a11y.atspi.Registry.EventListenerRegistered@/org/a11y/atspi/registry",
    "--broadcast=org.a11y.atspi.Registry=org.a11y.atspi.Registry.EventListenerDeregistered@/org/a11y/atspi/registry",
    "--call=org.a11y.atspi.Registry=org.a11y.atspi.Socket.Embed@/org/a11y/atspi/accessible/root",
    "--call=org.a11y.atspi.Registry=org.a11y.atspi.Socket.Unembed@/org/a11y/atspi/accessible/root",
    "--call=org.a11y.atspi.Registry=org.a11y.atspi.Registry.GetRegisteredEvents@/org/a11y/atspi/registry",
    "--call=org.a11y.atspi.Registry=org.a11y.atspi.DeviceEventController.GetKeystrokeListeners@/org/a11y/atspi/registry/deviceeventcontroller",
    "--call=org.a11y.atspi.Registry=org.a11y.atspi.DeviceEventController.GetDeviceEventListeners@/org/a11y/atspi/registry/deviceeventcontroller",
    "--call=org.a11y.atspi.Registry=org.a11y.atspi.DeviceEventController.NotifyListenersSync@/org/a11y/atspi/registry/deviceeventcontroller",
    "--call=org.a11y.atspi.Registry=org.a11y.atspi.DeviceEventController.NotifyListenersAsync@/org/a11y/atspi/registry/deviceeventcontroller",
];

/// What `xdg-dbus-proxy` takes to filter the desktop's accessibility bus into
/// `socket`, when the desktop runs one.
pub(super) fn filtered(socket: &Path) -> Option<Vec<OsString>> {
    let mut arguments = vec![
        address()?.into(),
        socket.into(),
        "--filter".into(),
        "--sloppy-names".into(),
    ];
    arguments.extend(RULES.iter().map(Into::into));
    Some(arguments)
}

/// Points the app and everything it starts at its filtered accessibility bus.
pub(super) fn point_at(socket: &Path) {
    // SAFETY: as for the session bus, set before the app starts its windows.
    unsafe { std::env::set_var(ADDRESS, format!("unix:path={}", socket.display())) };
}

fn address() -> Option<String> {
    if let Some(address) = std::env::var(ADDRESS)
        .ok()
        .filter(|address| !address.is_empty())
    {
        return Some(address);
    }
    sabine_platform::async_runtime().block_on(async {
        let connection = zbus::Connection::session().await.ok()?;
        let reply = connection
            .call_method(
                Some(LAUNCHER),
                LAUNCHER_PATH,
                Some(LAUNCHER),
                "GetAddress",
                &(),
            )
            .await
            .ok()?;
        reply.body().deserialize::<String>().ok()
    })
}
