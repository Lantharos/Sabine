use std::io::Write;

use crate::MenuRole;
use crate::bridge::QUIT_REQUESTED_LINE;
use crate::osr::host::native::OsrNativeHost;

impl OsrNativeHost {
    /// Does what `role` says. Returns false for a role the window leaves to
    /// the app, as `About` is outside macOS.
    pub(super) fn run_menu_role(&mut self, role: MenuRole) -> bool {
        match role {
            MenuRole::Quit => {
                let mut output = std::io::stdout().lock();
                let _ = writeln!(output, "{QUIT_REQUESTED_LINE}");
                let _ = output.flush();
            }
            MenuRole::Close => {
                self.menus.close_requested = true;
                self.proxy.wake_up();
            }
            MenuRole::Minimize => self.minimize_window(),
            MenuRole::Zoom => {
                if let Some(window) = &self.window {
                    window.set_maximized(!window.is_maximized());
                }
            }
            MenuRole::Fullscreen => {
                if let Some(window) = &self.window {
                    let fullscreen = window.fullscreen().is_none();
                    window.set_fullscreen(
                        fullscreen.then_some(winit::monitor::Fullscreen::Borderless(None)),
                    );
                }
            }
            role if role.edits() => {
                if !self.edit_sheet(role) {
                    self.send_control(format!("edit\t{}\n", role.name()));
                }
            }
            #[cfg(target_os = "macos")]
            MenuRole::About | MenuRole::Hide | MenuRole::HideOthers | MenuRole::ShowAll => {
                super::macos::run_app_role(role);
            }
            _ => return false,
        }
        true
    }
}
