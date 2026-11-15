use winit::event_loop::ActiveEventLoop;

use super::Flow;
use crate::osr::host::lifecycle::visibility::{activation_token_value, bool_control_value};
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{HostActivity, HostControl};

impl OsrNativeHost {
    /// Carries out what the app asked of its window.
    pub(super) fn apply_host_control(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        control: HostControl,
    ) -> Flow {
        match control {
            HostControl::Show => {
                self.ensure_window(event_loop);
                self.show_window("show");
            }
            HostControl::Visible(true) => {
                self.ensure_window(event_loop);
                self.show_window("visible");
            }
            HostControl::Hide => self.hide_window("hide"),
            HostControl::Visible(false) => self.hide_window("hidden"),
            HostControl::Focus(token) => self.activate_window(event_loop, token),
            HostControl::Regions(regions) => self.set_regions(regions),
            HostControl::SquareCorners(square) => self.set_square_corners(square),
            HostControl::Notify(notification) => self.show_notification(notification),
            HostControl::Print(files) => self.print_files(files),
            #[cfg(target_os = "windows")]
            HostControl::Badge(label) => self.taskbar.set_badge(label),
            #[cfg(target_os = "windows")]
            HostControl::Progress(progress) => self.taskbar.set_progress(progress),
            HostControl::Close => return self.close_window(event_loop),
            HostControl::Quit => {
                self.remember_size();
                self.forget_state();
                self.begin_close(event_loop);
                return Flow::Closed;
            }
            HostControl::ActivityBegin(activity) => self.begin_activity(activity),
            HostControl::ActivityEnd(activity) => self.end_activity(activity),
            HostControl::Permission(id, decision) => self.app_decided_permission(id, decision),
            HostControl::Dialog(id, dialog) => self.show_app_dialog(id, dialog),
            HostControl::ContextMenu(items) => self.set_context_menu_items(items),
            HostControl::AppMenu(items) => self.set_app_menu(items),
        }
        Flow::Continue
    }
}

pub(in crate::osr::host) fn host_control_from_parts(
    command: &str,
    value: &str,
) -> Option<HostControl> {
    match command {
        "visible" => bool_control_value(value).map(HostControl::Visible),
        "show" => Some(HostControl::Show),
        "hide" => Some(HostControl::Hide),
        "close" => Some(HostControl::Close),
        "quit" => Some(HostControl::Quit),
        "focus" => Some(HostControl::Focus(activation_token_value(Some(
            value.to_string(),
        )))),
        "notify" => crate::osr::protocol::notification_from_json(value).map(HostControl::Notify),
        "print" => serde_json::from_str(value).ok().map(HostControl::Print),
        #[cfg(target_os = "windows")]
        "badge" => serde_json::from_str(value).ok().map(HostControl::Badge),
        #[cfg(target_os = "windows")]
        "progress" => serde_json::from_str(value)
            .ok()
            .and_then(|progress| crate::desktop::progress_from_json(progress).ok())
            .map(HostControl::Progress),
        "activity.begin" => activity_control_value(value).map(HostControl::ActivityBegin),
        "activity.end" => activity_control_value(value).map(HostControl::ActivityEnd),
        "regions" => serde_json::from_str(value).ok().map(|regions| {
            HostControl::Regions(crate::osr::protocol::regions_from_json(Some(&regions)))
        }),
        "square-corners" => bool_control_value(value).map(HostControl::SquareCorners),
        "permission" => permission_control_value(value),
        "dialog" => {
            let dialog = serde_json::from_str::<serde_json::Value>(value).ok()?;
            let id = dialog.get("id")?.as_u64()?;
            Some(HostControl::Dialog(
                id,
                serde_json::from_value(dialog).ok()?,
            ))
        }
        "context-menu" => serde_json::from_str(value)
            .ok()
            .map(HostControl::ContextMenu),
        "app-menu" => serde_json::from_str(value).ok().map(HostControl::AppMenu),
        _ => None,
    }
}

fn activity_control_value(value: &str) -> Option<HostActivity> {
    let value = serde_json::from_str::<serde_json::Value>(value).ok()?;
    Some(HostActivity {
        id: value.get("id")?.as_str()?.to_string(),
        keeps_running: value
            .get("keepRunning")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
    })
}

fn permission_control_value(value: &str) -> Option<HostControl> {
    let value = serde_json::from_str::<serde_json::Value>(value).ok()?;
    let decision = match value.get("decision")?.as_str()? {
        "allow" => crate::PermissionDecision::Allow,
        "deny" => crate::PermissionDecision::Deny,
        "ask" => crate::PermissionDecision::Ask,
        _ => return None,
    };
    let id = u32::try_from(value.get("id")?.as_u64()?).ok()?;
    Some(HostControl::Permission(id, decision))
}
