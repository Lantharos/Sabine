use wayland_protocols::xdg::dialog::v1::client::{
    xdg_dialog_v1::XdgDialogV1, xdg_wm_dialog_v1::XdgWmDialogV1,
};
use wayland_protocols::xdg::foreign::zv2::client::{
    zxdg_imported_v2::ZxdgImportedV2, zxdg_importer_v2::ZxdgImporterV2,
};
use winit::window::Window;

use super::wayland::Client;

const WAYLAND_PREFIX: &str = "wayland:";

/// Another app's window, imported through `zxdg_foreign_v2`, that a window
/// stays over as its dialog. The window follows its parent for as long as
/// this lasts.
pub struct ForeignParent {
    imported: ZxdgImportedV2,
    dialog: Option<XdgDialogV1>,
    client: Client,
}

impl ForeignParent {
    /// Puts `window` over the window `identifier` names, as
    /// [`ParentWindow::identifier`](super::ParentWindow::identifier) and the
    /// desktop portal's `parent_window` argument do. A modal window also
    /// keeps people from using its parent, on compositors that offer
    /// `xdg_wm_dialog_v1`.
    pub fn attach(window: &dyn Window, identifier: &str, modal: bool) -> Result<Self, String> {
        let handle = identifier
            .strip_prefix(WAYLAND_PREFIX)
            .filter(|handle| !handle.is_empty())
            .ok_or("The parent is not a Wayland window")?;
        let client = Client::connect(window)?;
        let surface = client.surface(window)?;
        let importer = client
            .globals
            .bind::<ZxdgImporterV2, _, _>(&client.handle(), 1..=1, ())
            .map_err(|_| "This desktop cannot place windows over other apps' windows")?;
        let imported = importer.import_toplevel(handle.to_string(), &client.handle(), ());
        importer.destroy();
        imported.set_parent_of(&surface);
        let dialog = modal.then(|| Self::modal_dialog(&client, window)).flatten();
        client.flush();
        Ok(Self {
            imported,
            dialog,
            client,
        })
    }

    fn modal_dialog(client: &Client, window: &dyn Window) -> Option<XdgDialogV1> {
        let toplevel = client.toplevel(window).ok()?;
        let manager = client
            .globals
            .bind::<XdgWmDialogV1, _, _>(&client.handle(), 1..=1, ())
            .ok()?;
        let dialog = manager.get_xdg_dialog(&toplevel, &client.handle(), ());
        manager.destroy();
        dialog.set_modal();
        Some(dialog)
    }
}

impl Drop for ForeignParent {
    fn drop(&mut self) {
        if let Some(dialog) = self.dialog.take() {
            dialog.destroy();
        }
        self.imported.destroy();
        self.client.flush();
    }
}
