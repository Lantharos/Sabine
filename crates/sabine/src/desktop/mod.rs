#[cfg(any(target_os = "windows", target_os = "macos"))]
mod hotkeys;
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
#[cfg(unix)]
mod unix_instance;

use std::thread::{self, JoinHandle};

use sabine_platform::{
    AutostartEntry, DeepLinkRegistration, GlobalShortcutFailure, GlobalShortcutRegistration,
    NativeMessagingHost, PlatformEvent, SingleInstancePolicy, TrayIcon,
};

use platform::{
    GlobalShortcuts, SingleInstanceGuard, UiThread, register_deep_links,
    register_native_messaging_host, write_autostart_entry,
};
pub use tray::TrayHandle;
pub(crate) use tray::UPDATE_COMMAND as TRAY_UPDATE_COMMAND;

pub(crate) const INSTANCE_ALREADY_RUNNING: &str = "another instance is already running";

type EventQueue = crossbeam_channel::Sender<PlatformEvent>;

/// The desktop integrations an app process owns: its tray icon, global
/// shortcuts, single-instance endpoint and the events they produce.
pub struct DesktopServiceState {
    event_receiver: crossbeam_channel::Receiver<PlatformEvent>,
    tray: Option<TrayHandle>,
    _shortcuts: Option<GlobalShortcuts>,
    _single_instance: Option<SingleInstanceGuard>,
    #[cfg(target_os = "macos")]
    _app_events: platform::AppEvents,
    ui_thread: UiThread,
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
    pub fn take_events(&self) -> Vec<PlatformEvent> {
        self.event_receiver.try_iter().collect()
    }

    pub fn tray(&self) -> Option<TrayHandle> {
        self.tray.clone()
    }
}

impl Drop for DesktopServiceState {
    fn drop(&mut self) {
        self.ui_thread.queue().run(tray::remove);
    }
}

pub fn apply_desktop_services(
    tray_icon: Option<&TrayIcon>,
    autostart: &[AutostartEntry],
    global_shortcuts: &[GlobalShortcutRegistration],
    deep_links: &[DeepLinkRegistration],
    native_messaging_hosts: &[NativeMessagingHost],
    single_instance_id: Option<&str>,
    single_instance_policy: Option<SingleInstancePolicy>,
) -> Result<DesktopServiceState, String> {
    let (event_sender, event_receiver) = crossbeam_channel::unbounded();
    let single_instance = single_instance_policy
        .filter(|policy| *policy != SingleInstancePolicy::AllowMultiple)
        .map(|policy| {
            SingleInstanceGuard::acquire(single_instance_id, policy, event_sender.clone())
        })
        .transpose()?;
    for entry in autostart {
        write_autostart_entry(entry)?;
    }
    for registration in deep_links {
        register_deep_links(registration)?;
    }
    for host in native_messaging_hosts {
        register_native_messaging_host(host)?;
    }
    #[cfg(target_os = "macos")]
    let app_events = platform::AppEvents::install(event_sender.clone());
    let ui_thread = UiThread::start()?;
    let tray = tray_icon.map(|icon| TrayHandle::spawn(ui_thread.queue(), icon, &event_sender));
    let shortcuts = (!global_shortcuts.is_empty()).then(|| {
        GlobalShortcuts::register(&ui_thread.queue(), global_shortcuts, event_sender.clone())
    });
    Ok(DesktopServiceState {
        event_receiver,
        tray,
        _shortcuts: shortcuts,
        _single_instance: single_instance,
        #[cfg(target_os = "macos")]
        _app_events: app_events,
        ui_thread,
    })
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
