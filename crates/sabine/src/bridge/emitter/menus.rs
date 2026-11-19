use super::BridgeEventEmitter;
use super::lock;
use crate::launch::browser::HOST_CONTROL_PREFIX;
use crate::{MenuItem, WindowId};

/// What a window host writes when its page sets the app menu bar, which
/// every other window then shows too.
pub(crate) const APP_MENU_LINE: &str = "SABINE_APP_MENU";

impl BridgeEventEmitter {
    /// Replaces the app menu bar of every window, also the ones opened
    /// later. On macOS an empty menu brings back the standard app, Edit,
    /// View and Window menus.
    pub fn set_app_menu(&self, items: &[MenuItem]) -> bool {
        let control = app_menu_control(items);
        let mut current = lock(&self.app_menu);
        *current = Some(control.clone());
        self.deliver(None, control, None)
    }

    /// Shows the app menu bar that one window's page set in every other
    /// window, and in the windows opened later.
    pub(crate) fn share_app_menu(&self, from: WindowId, items: &str) {
        let control = format!("{HOST_CONTROL_PREFIX}\tapp-menu\t{items}");
        let mut current = lock(&self.app_menu);
        self.deliver_to_others(from, &control);
        *current = Some(control);
    }
}

pub(crate) fn app_menu_control(items: &[MenuItem]) -> String {
    format!(
        "{HOST_CONTROL_PREFIX}\tapp-menu\t{}",
        serde_json::json!(items)
    )
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::BufRead;
    use std::process::{Command, Stdio};

    use sabine_bridge::ActivityRegistry;

    use super::*;
    use crate::bridge::writer::BridgeWriter;

    #[test]
    fn windows_opened_later_get_the_app_menu() {
        let emitter = BridgeEventEmitter::default();
        let menu = [MenuItem::submenu("File", vec![MenuItem::new("new", "New")])];
        emitter.set_app_menu(&menu);
        let mut window = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("cat");
        let writer = BridgeWriter::spawn(window.stdin.take().expect("stdin"));
        emitter.attach(window.id(), writer, &ActivityRegistry::default());
        let mut line = String::new();
        std::io::BufReader::new(window.stdout.as_mut().expect("stdout"))
            .read_line(&mut line)
            .expect("line");
        assert_eq!(line, format!("{}\n", app_menu_control(&menu)));
        let _ = window.kill();
        let _ = window.wait();
    }
}
