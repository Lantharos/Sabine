use wayland_protocols::xdg::foreign::zv2::client::{
    zxdg_exported_v2::ZxdgExportedV2, zxdg_exporter_v2::ZxdgExporterV2,
};
use winit::window::Window;

use super::wayland::Client;

/// A window exported through `zxdg_foreign_v2`, which lets the desktop portal
/// keep its dialogs over it while the export lasts.
pub struct ParentWindow {
    exported: ZxdgExportedV2,
    handle: String,
    client: Client,
}

impl ParentWindow {
    pub fn export(window: &dyn Window) -> Result<Self, String> {
        let mut client = Client::connect(window)?;
        let surface = client.surface(window)?;
        let exporter = client
            .globals
            .bind::<ZxdgExporterV2, _, _>(&client.handle(), 1..=1, ())
            .map_err(|_| "This desktop cannot share its windows with other apps")?;
        let exported = exporter.export_toplevel(&surface, &client.handle(), ());
        exporter.destroy();
        let handle = client
            .roundtrip()
            .ok()
            .and_then(|()| client.replies.exported_handle.take());
        let Some(handle) = handle else {
            exported.destroy();
            return Err("The compositor did not export the window".to_string());
        };
        Ok(Self {
            exported,
            handle,
            client,
        })
    }

    /// The window as the portal's `parent_window` argument names it.
    pub fn identifier(&self) -> String {
        format!("wayland:{}", self.handle)
    }
}

impl Drop for ParentWindow {
    fn drop(&mut self) {
        self.exported.destroy();
        self.client.flush();
    }
}
