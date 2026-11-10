//! Connections to the compositor of a window, made on the display winit
//! already opened, for the protocols Sabine uses beside winit.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalList, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{
    wl_compositor::WlCompositor, wl_region::WlRegion, wl_registry::WlRegistry, wl_seat::WlSeat,
    wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1},
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
    zwp_keyboard_shortcuts_inhibitor_v1::ZwpKeyboardShortcutsInhibitorV1,
};
use wayland_protocols::xdg::dialog::v1::client::{
    xdg_dialog_v1::XdgDialogV1, xdg_wm_dialog_v1::XdgWmDialogV1,
};
use wayland_protocols::xdg::foreign::zv2::client::{
    zxdg_exported_v2::{self, ZxdgExportedV2},
    zxdg_exporter_v2::ZxdgExporterV2,
    zxdg_imported_v2::ZxdgImportedV2,
    zxdg_importer_v2::ZxdgImporterV2,
};
use wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
use winit::platform::wayland::WindowExtWayland;
use winit::window::Window;

use super::kestrel::{
    kestrel_window_manager_v1::KestrelWindowManagerV1,
    kestrel_window_state_v1::KestrelWindowStateV1,
};

/// The events Sabine waits for on its own connections.
#[derive(Default)]
pub(crate) struct Replies {
    pub(crate) blur_supported: bool,
    pub(crate) exported_handle: Option<String>,
}

/// A connection to a window's compositor with the globals it advertises.
/// Objects made through it must go before it, since it takes the proxies it
/// still knows with it.
pub(crate) struct Client {
    pub(crate) connection: Connection,
    pub(crate) globals: GlobalList,
    pub(crate) queue: EventQueue<Replies>,
    pub(crate) replies: Replies,
}

impl Client {
    pub(crate) fn connect(window: &dyn Window) -> Result<Self, String> {
        let RawDisplayHandle::Wayland(display) = window
            .display_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("The window has no Wayland display".to_string());
        };
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<Replies>(&connection)
            .map_err(|error| format!("Could not read the Wayland compositor globals: {error}"))?;
        Ok(Self {
            connection,
            globals,
            queue,
            replies: Replies::default(),
        })
    }

    pub(crate) fn handle(&self) -> QueueHandle<Replies> {
        self.queue.handle()
    }

    pub(crate) fn surface(&self, window: &dyn Window) -> Result<WlSurface, String> {
        let RawWindowHandle::Wayland(surface) = window
            .window_handle()
            .map_err(|error| error.to_string())?
            .as_raw()
        else {
            return Err("The window has no Wayland surface".to_string());
        };
        self.foreign(surface.surface.as_ptr())
    }

    pub(crate) fn toplevel(&self, window: &dyn Window) -> Result<XdgToplevel, String> {
        let toplevel = window
            .xdg_toplevel()
            .ok_or("The window has no Wayland toplevel")?;
        self.foreign(toplevel.as_ptr())
    }

    /// Waits until the compositor has answered every request sent so far.
    pub(crate) fn roundtrip(&mut self) -> Result<(), String> {
        self.queue
            .roundtrip(&mut self.replies)
            .map(drop)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn flush(&self) {
        let _ = self.connection.flush();
    }

    /// The version of a global the compositor advertises.
    pub(crate) fn version_of(&self, interface: &str) -> Option<u32> {
        self.globals.contents().with_list(|globals| {
            globals
                .iter()
                .find(|global| global.interface == interface)
                .map(|global| global.version)
        })
    }

    fn foreign<I: Proxy>(&self, pointer: *mut std::ffi::c_void) -> Result<I, String> {
        let id = unsafe { ObjectId::from_ptr(I::interface(), pointer.cast()) }
            .map_err(|error| error.to_string())?;
        I::from_id(&self.connection, id).map_err(|error| error.to_string())
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Replies {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for Replies {
    fn event(
        replies: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            replies.blur_supported = flags.into_result().is_ok_and(|flags| {
                flags.contains(ext_background_effect_manager_v1::Capability::Blur)
            });
        }
    }
}

impl Dispatch<ZxdgExportedV2, ()> for Replies {
    fn event(
        replies: &mut Self,
        _: &ZxdgExportedV2,
        event: zxdg_exported_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zxdg_exported_v2::Event::Handle { handle } = event {
            replies.exported_handle = Some(handle);
        }
    }
}

delegate_noop!(Replies: ignore WlCompositor);
delegate_noop!(Replies: ignore WlRegion);
delegate_noop!(Replies: ignore WlSeat);
delegate_noop!(Replies: ignore ExtBackgroundEffectSurfaceV1);
delegate_noop!(Replies: ignore ZwpKeyboardShortcutsInhibitManagerV1);
delegate_noop!(Replies: ignore ZwpKeyboardShortcutsInhibitorV1);
delegate_noop!(Replies: ignore ZxdgExporterV2);
delegate_noop!(Replies: ignore ZxdgImporterV2);
delegate_noop!(Replies: ignore ZxdgImportedV2);
delegate_noop!(Replies: ignore XdgWmDialogV1);
delegate_noop!(Replies: ignore XdgDialogV1);
delegate_noop!(Replies: ignore KestrelWindowManagerV1);
delegate_noop!(Replies: ignore KestrelWindowStateV1);
