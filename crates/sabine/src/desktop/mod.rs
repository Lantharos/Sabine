#[cfg(any(target_os = "windows", target_os = "macos"))]
mod hotkeys;
mod instance;
mod native_messaging;
pub(crate) mod open_urls;
#[cfg(target_os = "linux")]
#[path = "linux/mod.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos/mod.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows/mod.rs"]
mod platform;
mod shortcut_key;
mod tray;

use std::thread::{self, JoinHandle};

use sabine_platform::{
    GlobalShortcutFailure, GlobalShortcutRegistration, PlatformEvent, SingleInstancePolicy,
};

use instance::SingleInstanceGuard;
#[cfg(target_os = "linux")]
pub(crate) use instance::startup_activation_token;
#[cfg(target_os = "linux")]
use platform::ShortcutHost;
#[cfg(not(target_os = "linux"))]
use platform::UiThread;
#[cfg(target_os = "macos")]
pub(crate) use platform::desktop_wait;
#[cfg(target_os = "windows")]
pub(crate) use platform::with_app_icon;
use platform::{
    GlobalShortcuts, register_deep_links, register_native_messaging_host, write_autostart_entry,
};
pub use tray::TrayHandle;
pub(crate) use tray::UPDATE_COMMAND as TRAY_UPDATE_COMMAND;

use crate::osr::launch::require_app_id;
use crate::window::config::SabineWindowConfig;
use crate::{SabineError, SabineResult};

type EventQueue = crossbeam_channel::Sender<PlatformEvent>;

/// The app's claim on its single instance, made before anything else at
/// launch so a second launch hands off at once, and the queue its desktop
/// events arrive on.
pub(crate) struct DesktopClaim {
    event_sender: EventQueue,
    event_receiver: crossbeam_channel::Receiver<PlatformEvent>,
    single_instance: Option<SingleInstanceGuard>,
}

impl DesktopClaim {
    /// Fails with [`SabineError::InstanceAlreadyRunning`] once this launch
    /// was handed to the running instance.
    pub(crate) fn acquire(config: &SabineWindowConfig) -> SabineResult<Self> {
        let services = &config.desktop_services;
        let (event_sender, event_receiver) = crossbeam_channel::unbounded();
        let single_instance = services
            .single_instance_policy
            .filter(|policy| *policy != SingleInstancePolicy::AllowMultiple)
            .map(|policy| {
                let id = match &services.single_instance_id {
                    Some(id) => id.as_str(),
                    None => require_app_id(config)?,
                };
                SingleInstanceGuard::acquire(id, policy, event_sender.clone())
            })
            .transpose()?;
        Ok(Self {
            event_sender,
            event_receiver,
            single_instance,
        })
    }

    /// Registers the app with the desktop and shows its tray icon and
    /// shortcuts.
    pub(crate) fn start(self, config: &SabineWindowConfig) -> SabineResult<DesktopServiceState> {
        let services = &config.desktop_services;
        let failed = |message: String| SabineError::CreationFailed { message };
        for entry in &services.autostart {
            write_autostart_entry(entry).map_err(failed)?;
        }
        for registration in &services.deep_links {
            register_deep_links(registration).map_err(failed)?;
        }
        for host in &services.native_messaging_hosts {
            register_native_messaging_host(host).map_err(failed)?;
        }
        let events = self.event_sender;
        #[cfg(target_os = "macos")]
        let app_events = platform::AppEvents::install(events.clone());
        #[cfg(target_os = "linux")]
        let shortcut_host = ShortcutHost::new(require_app_id(config)?, &config.title);
        #[cfg(target_os = "linux")]
        let (tray, shortcuts) = (
            services
                .tray_icon
                .as_ref()
                .map(|icon| TrayHandle::spawn(icon, &events)),
            (!services.global_shortcuts.is_empty()).then(|| {
                GlobalShortcuts::register(&services.global_shortcuts, shortcut_host, events.clone())
            }),
        );
        #[cfg(not(target_os = "linux"))]
        let ui_thread = UiThread::start().map_err(failed)?;
        #[cfg(not(target_os = "linux"))]
        let (tray, shortcuts) = (
            services
                .tray_icon
                .as_ref()
                .map(|icon| TrayHandle::spawn(ui_thread.queue(), icon, &events)),
            (!services.global_shortcuts.is_empty()).then(|| {
                GlobalShortcuts::register(
                    &ui_thread.queue(),
                    &services.global_shortcuts,
                    events.clone(),
                )
            }),
        );
        Ok(DesktopServiceState {
            event_receiver: self.event_receiver,
            tray,
            _shortcuts: shortcuts,
            _single_instance: self.single_instance,
            #[cfg(target_os = "macos")]
            _app_events: app_events,
            #[cfg(not(target_os = "linux"))]
            _ui_thread: ui_thread,
        })
    }
}

/// The desktop integrations an app process owns: its tray icon, global
/// shortcuts, single-instance endpoint and the events they produce.
pub struct DesktopServiceState {
    event_receiver: crossbeam_channel::Receiver<PlatformEvent>,
    tray: Option<TrayHandle>,
    _shortcuts: Option<GlobalShortcuts>,
    _single_instance: Option<SingleInstanceGuard>,
    #[cfg(target_os = "macos")]
    _app_events: platform::AppEvents,
    #[cfg(not(target_os = "linux"))]
    _ui_thread: UiThread,
}

impl std::fmt::Debug for DesktopServiceState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DesktopServiceState")
            .field("queued_events", &self.event_receiver.len())
            .field("tray", &self.tray.is_some())
            .finish()
    }
}

impl DesktopServiceState {
    pub fn tray(&self) -> Option<TrayHandle> {
        self.tray.clone()
    }
}

impl Drop for DesktopServiceState {
    fn drop(&mut self) {
        if let Some(tray) = &self.tray {
            tray.remove();
        }
    }
}

pub fn start_desktop_event_forwarder(
    services: &DesktopServiceState,
    stop: crossbeam_channel::Receiver<()>,
    mut emit: impl FnMut(PlatformEvent) + Send + 'static,
) -> JoinHandle<()> {
    let events = services.event_receiver.clone();
    thread::spawn(move || {
        loop {
            crossbeam_channel::select! {
                recv(stop) -> _ => break,
                recv(events) -> event => match event {
                    Ok(event) => emit(event),
                    Err(_) => break,
                },
            }
        }
    })
}

/// Reports a shortcut the desktop refused to the app's diagnostics and its
/// pages, leaving the others working.
fn report_shortcut_failure(
    events: &EventQueue,
    registration: &GlobalShortcutRegistration,
    message: impl Into<String>,
) {
    let message = message.into();
    sabine_runtime::report_error(
        "desktop",
        format!(
            "global shortcut {} was not registered: {message}",
            registration.id
        ),
    );
    let _ = events.send(PlatformEvent::GlobalShortcutFailed(GlobalShortcutFailure {
        id: registration.id.clone(),
        action: registration.action.clone(),
        message,
    }));
}

/// `value` as a file, socket or desktop identifier: letters, digits, dots,
/// hyphens and underscores, with anything else turned into underscores.
pub(crate) fn sanitize_id(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if sanitized.is_empty() {
        "app".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_id;

    #[test]
    fn sanitized_ids_keep_safe_characters() {
        assert_eq!(sanitize_id("com.sabine.notes"), "com.sabine.notes");
        assert_eq!(sanitize_id("a/b c"), "a_b_c");
        assert_eq!(sanitize_id("@@@"), "app");
    }
}
