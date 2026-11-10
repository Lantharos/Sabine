use sabine_platform::shell::{ForeignParent, ParentWindow};
use winit::window::Window;

use super::OsrNativeHost;

impl OsrNativeHost {
    /// The window as portal requests name their parent, so the desktop
    /// places their dialogs over it. The window is exported once and stays
    /// exported while it is shown.
    pub(in crate::osr::host) fn portal_parent(&mut self) -> String {
        if self.exported_window.is_none()
            && let Some(window) = &self.window
        {
            match ParentWindow::export(window.as_ref()) {
                Ok(exported) => self.exported_window = Some(exported),
                Err(error) => sabine_runtime::report_error(
                    "portal",
                    format!("the desktop's dialogs open without their window: {error}"),
                ),
            }
        }
        self.exported_window
            .as_ref()
            .map(ParentWindow::identifier)
            .unwrap_or_default()
    }

    /// Keeps the window over the window of another app it was opened for.
    pub(super) fn attach_foreign_parent(&mut self, window: &dyn Window) {
        let Some(parent) = &self.config.parent_window else {
            return;
        };
        match ForeignParent::attach(window, parent, self.config.modal) {
            Ok(parent) => self.foreign_parent = Some(parent),
            Err(error) => sabine_runtime::report_error(
                "window",
                format!("the window opens without its parent: {error}"),
            ),
        }
    }
}
