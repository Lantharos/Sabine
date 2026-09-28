#[cfg(windows)]
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use sabine_bridge::INHIBIT_SHORTCUTS_COMMAND;
#[cfg(not(target_os = "macos"))]
use sabine_platform::ShortcutInhibitor;
#[cfg(windows)]
use sabine_platform::SystemKey;
use serde_json::Value;

use crate::osr::host::native::OsrNativeHost;

#[cfg(not(target_os = "macos"))]
#[derive(Default)]
pub(in crate::osr::host) struct ShortcutInhibition {
    requested: bool,
    inhibitor: Option<ShortcutInhibitor>,
    #[cfg(windows)]
    system_keys: Rc<RefCell<VecDeque<SystemKey>>>,
}

impl OsrNativeHost {
    /// Answer the bridge commands that act on this window. Returns false for
    /// every other command so it reaches the app.
    pub(in crate::osr::host) fn answer_window_bridge_request(&mut self, line: &str) -> bool {
        let parts = line.splitn(6, '\t').collect::<Vec<_>>();
        let [
            _,
            browser_id,
            request_id,
            _,
            INHIBIT_SHORTCUTS_COMMAND,
            params,
        ] = parts[..]
        else {
            return false;
        };
        let enabled = serde_json::from_str::<Value>(params)
            .ok()
            .and_then(|params| params.get("enabled")?.as_bool());
        let result = match enabled {
            Some(enabled) => self.set_shortcuts_inhibited(enabled),
            None => Err("inhibitShortcuts expects a boolean".to_string()),
        };
        self.send_bridge_response(browser_id, request_id, result.map(|()| Value::Null));
        true
    }

    #[cfg(target_os = "macos")]
    fn set_shortcuts_inhibited(&mut self, _enabled: bool) -> Result<(), String> {
        Err("macOS does not let apps inhibit its keyboard shortcuts".to_string())
    }

    #[cfg(not(target_os = "macos"))]
    fn set_shortcuts_inhibited(&mut self, enabled: bool) -> Result<(), String> {
        self.shortcuts.requested = enabled;
        if !enabled {
            self.shortcuts.inhibitor = None;
            return Ok(());
        }
        self.restore_shortcut_inhibitor()
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::osr::host) fn restore_shortcut_inhibitor(&mut self) -> Result<(), String> {
        if !self.shortcuts.requested || self.shortcuts.inhibitor.is_some() {
            return Ok(());
        }
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        #[cfg(target_os = "linux")]
        let inhibitor = ShortcutInhibitor::new(window.as_ref(), self.focused);
        #[cfg(windows)]
        let inhibitor = {
            let system_keys = Rc::clone(&self.shortcuts.system_keys);
            let proxy = self.proxy.clone();
            ShortcutInhibitor::new(window.as_ref(), move |key| {
                system_keys.borrow_mut().push_back(key);
                proxy.wake_up();
            })
        };
        match inhibitor {
            Ok(inhibitor) => {
                self.shortcuts.inhibitor = Some(inhibitor);
                Ok(())
            }
            Err(error) => {
                self.shortcuts.requested = false;
                Err(error)
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub(in crate::osr::host) fn release_shortcut_inhibitor(&mut self) {
        self.shortcuts.inhibitor = None;
    }

    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn sync_shortcut_inhibitor_focus(&mut self) {
        if let Some(inhibitor) = &mut self.shortcuts.inhibitor {
            inhibitor.set_focused(self.focused);
        }
    }

    #[cfg(windows)]
    pub(in crate::osr::host) fn forward_system_keys(&self) {
        let keys = std::mem::take(&mut *self.shortcuts.system_keys.borrow_mut());
        for key in keys {
            self.send_system_key(key);
        }
    }
}
