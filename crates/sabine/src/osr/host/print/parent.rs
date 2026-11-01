use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry::WlRegistry, wl_surface::WlSurface},
};
use wayland_protocols::xdg::foreign::zv2::client::{
    zxdg_exported_v2::{self, ZxdgExportedV2},
    zxdg_exporter_v2::ZxdgExporterV2,
};
use winit::window::Window;

use super::portal::Requester;
use crate::osr::host::native::OsrNativeHost;

/// The window, exported for the desktop's portal dialogs to sit on top of.
pub(in crate::osr::host) struct ExportedWindow {
    queue: EventQueue<Handle>,
    exported: ZxdgExportedV2,
    identifier: String,
}

#[derive(Default)]
struct Handle(Option<String>);

impl ExportedWindow {
    fn export(window: &dyn Window) -> Result<Self, String> {
        let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(surface)) = (
            window
                .display_handle()
                .map_err(|error| error.to_string())?
                .as_raw(),
            window
                .window_handle()
                .map_err(|error| error.to_string())?
                .as_raw(),
        ) else {
            return Err("the window is not on Wayland".to_string());
        };
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let surface =
            unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.surface.as_ptr().cast()) }
                .and_then(|id| WlSurface::from_id(&connection, id))
                .map_err(|error| error.to_string())?;
        let (globals, mut queue) =
            registry_queue_init::<Handle>(&connection).map_err(|error| error.to_string())?;
        let handle = queue.handle();
        let exporter: ZxdgExporterV2 = globals
            .bind(&handle, 1..=1, ())
            .map_err(|_| "the desktop can't place dialogs over apps".to_string())?;
        let exported = exporter.export_toplevel(&surface, &handle, ());
        exporter.destroy();
        let mut received = Handle::default();
        let handle = loop {
            if let Some(handle) = received.0.take() {
                break handle;
            }
            queue
                .blocking_dispatch(&mut received)
                .map_err(|error| error.to_string())?;
        };
        Ok(Self {
            queue,
            exported,
            identifier: format!("wayland:{handle}"),
        })
    }
}

impl Drop for ExportedWindow {
    fn drop(&mut self) {
        self.exported.destroy();
        let _ = self.queue.flush();
    }
}

impl OsrNativeHost {
    /// The window as portal requests name their parent, so the desktop
    /// places their dialogs over it.
    fn portal_parent(&mut self) -> String {
        if self.exported_window.is_none()
            && let Some(window) = &self.window
        {
            match ExportedWindow::export(window.as_ref()) {
                Ok(exported) => self.exported_window = Some(exported),
                Err(error) => sabine_runtime::report_error(
                    "print",
                    format!("the print dialog opens without its window: {error}"),
                ),
            }
        }
        self.exported_window
            .as_ref()
            .map(|exported| exported.identifier.clone())
            .unwrap_or_default()
    }

    pub(super) fn print_requester(&mut self) -> Requester {
        Requester {
            app_id: self.config.app_id.clone(),
            parent: self.portal_parent(),
        }
    }
}

impl Dispatch<ZxdgExportedV2, ()> for Handle {
    fn event(
        received: &mut Self,
        _exported: &ZxdgExportedV2,
        event: zxdg_exported_v2::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let zxdg_exported_v2::Event::Handle { handle } = event {
            received.0 = Some(handle);
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Handle {
    fn event(
        _received: &mut Self,
        _registry: &WlRegistry,
        _event: <WlRegistry as Proxy>::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Handle: ZxdgExporterV2);
