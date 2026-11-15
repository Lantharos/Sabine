mod store;

use std::collections::{HashMap, VecDeque};
use std::io::Write;

use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::PermissionAsk;
use crate::window::{PERMISSION_REQUEST_LINE, PermissionDecision};

/// Permissions people decide each time, since what they share changes.
const ASKED_EACH_TIME: [&str; 1] = ["screen"];

/// The window's permission requests: those the app decides first, and those
/// waiting for people, the first of them shown.
pub(in crate::osr::host) struct PagePermissions {
    store: store::PermissionStore,
    with_app: HashMap<u32, PermissionAsk>,
    waiting: VecDeque<PermissionAsk>,
}

impl PagePermissions {
    pub(in crate::osr::host) fn new(app_id: Option<&str>) -> Self {
        Self {
            store: store::PermissionStore::new(app_id),
            with_app: HashMap::new(),
            waiting: VecDeque::new(),
        }
    }

    pub(in crate::osr::host) fn shown(&self) -> Option<&PermissionAsk> {
        self.waiting.front()
    }
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn ask_permission(&mut self, ask: PermissionAsk) {
        if !self.config.asks_permissions {
            self.decide_permission(ask);
            return;
        }
        let request = serde_json::json!({
            "id": ask.id,
            "origin": ask.origin,
            "permissions": ask.permissions,
            "guest": ask.guest,
        });
        let mut output = std::io::stdout().lock();
        let _ = writeln!(output, "{PERMISSION_REQUEST_LINE}\t{request}");
        let _ = output.flush();
        self.permissions.with_app.insert(ask.id, ask);
    }

    pub(in crate::osr::host) fn app_decided_permission(
        &mut self,
        id: u32,
        decision: PermissionDecision,
    ) {
        let Some(ask) = self.permissions.with_app.remove(&id) else {
            return;
        };
        match decision {
            PermissionDecision::Allow => self.answer_permission(id, true),
            PermissionDecision::Deny => self.answer_permission(id, false),
            PermissionDecision::Ask => self.decide_permission(ask),
        }
    }

    pub(in crate::osr::host) fn permission_withdrawn(&mut self, id: u32) {
        self.permissions.with_app.remove(&id);
        let shown = self.permissions.shown().map(|ask| ask.id);
        self.permissions.waiting.retain(|ask| ask.id != id);
        if shown == Some(id) {
            self.show_permission_prompt();
        }
    }

    /// Drops the requests of a browser that went away, whose answers nothing
    /// waits for.
    pub(in crate::osr::host) fn forget_permission_requests(&mut self) {
        self.permissions.with_app.clear();
        self.permissions.waiting.clear();
        self.show_permission_prompt();
    }

    /// The person chose in the prompt the window shows.
    pub(in crate::osr::host) fn person_decided_permission(&mut self, allowed: bool) {
        let Some(ask) = self.permissions.waiting.pop_front() else {
            return;
        };
        let remembered = ask
            .permissions
            .iter()
            .filter(|permission| !ASKED_EACH_TIME.contains(&permission.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !remembered.is_empty() {
            self.permissions
                .store
                .remember(&ask.origin, &remembered, allowed);
        }
        self.answer_permission(ask.id, allowed);
        self.show_permission_prompt();
    }

    fn decide_permission(&mut self, ask: PermissionAsk) {
        if let Some(allowed) = self
            .permissions
            .store
            .decided(&ask.origin, &ask.permissions)
        {
            self.answer_permission(ask.id, allowed);
            return;
        }
        if the_desktop_asks(&ask) {
            self.answer_permission(ask.id, true);
            return;
        }
        self.permissions.waiting.push_back(ask);
        if self.permissions.waiting.len() == 1 {
            self.show_permission_prompt();
        }
    }

    fn answer_permission(&self, id: u32, allowed: bool) {
        self.send_control(format!("permission\t{id}\t{}\n", u8::from(allowed)));
    }
}

/// On Linux, the desktop's screen sharing portal asks people what to share.
fn the_desktop_asks(ask: &PermissionAsk) -> bool {
    cfg!(target_os = "linux") && ask.permissions.iter().all(|name| name == "screen")
}
