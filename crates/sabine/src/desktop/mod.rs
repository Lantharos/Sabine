#[cfg(any(target_os = "windows", target_os = "macos"))]
mod hotkeys;
mod instance;
mod launcher;
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
mod power;
mod shortcut_key;
mod tray;

use std::{
    fs, io,
    path::Path,
    sync::Arc,
    thread::{self, JoinHandle},
};

use sabine_platform::{
    AutostartEntry, DeepLinkRegistration, GlobalShortcutFailure, GlobalShortcutRegistration,
    NativeMessagingHost, PlatformEvent, PowerStatus, PowerWatcher, SingleInstancePolicy,
};

use instance::SingleInstanceGuard;
#[cfg(target_os = "linux")]
pub(crate) use instance::startup_activation_token;
pub(crate) use launcher::{Launcher, launcher_commands};
#[cfg(target_os = "windows")]
pub(crate) use launcher::{progress_from_json, progress_to_json};
#[cfg(target_os = "linux")]
use platform::ShortcutHost;
#[cfg(not(target_os = "linux"))]
use platform::UiThread;
#[cfg(target_os = "macos")]
pub(crate) use platform::desktop_wait;
use platform::{
    GlobalShortcuts, register_deep_links, register_native_messaging_host, write_autostart_entry,
};
#[cfg(target_os = "windows")]
pub(crate) use platform::{allow_foreground, with_app_icon};
pub(crate) use power::{IdleWatches, keep_awake};
pub use tray::TrayHandle;
pub(crate) use tray::UPDATE_COMMAND as TRAY_UPDATE_COMMAND;

use crate::osr::launch::require_app_id;
use crate::window::config::SabineWindowConfig;
use crate::{SabineError, SabineResult};

pub(crate) type EventQueue = crossbeam_channel::Sender<PlatformEvent>;

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
    /// shortcuts. Development runs leave the desktop's registrations alone.
    pub(crate) fn start(self, config: &SabineWindowConfig) -> SabineResult<DesktopServiceState> {
        let services = &config.desktop_services;
        let failed = |message: String| SabineError::CreationFailed { message };
        let registers = !config.dev_mode();
        let autostart = Autostart {
            entry: services.autostart.clone(),
            registers,
        };
        if let Some(entry) = &autostart.entry {
            autostart.set_enabled(entry.enabled).map_err(failed)?;
        }
        if registers {
            register_with_desktop(Registrations {
                deep_links: services.deep_links.clone(),
                hosts: services.native_messaging_hosts.clone(),
                #[cfg(target_os = "windows")]
                actions: services.app_actions.clone(),
            });
        }
        let events = self.event_sender;
        let power = PowerWatcher::start(Arc::new({
            let events = events.clone();
            move |event| {
                let _ = events.send(PlatformEvent::Power(event));
            }
        }));
        #[cfg(target_os = "macos")]
        let app_events = platform::AppEvents::install(platform::AppEventTargets {
            events: events.clone(),
            actions: services.app_actions.clone(),
            policy: services
                .single_instance_policy
                .filter(|policy| *policy != SingleInstancePolicy::AllowMultiple)
                .unwrap_or(SingleInstancePolicy::FocusExisting),
        });
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
            events,
            power,
            autostart,
            tray,
            _shortcuts: shortcuts,
            single_instance: self.single_instance,
            #[cfg(target_os = "macos")]
            _app_events: app_events,
            #[cfg(not(target_os = "linux"))]
            _ui_thread: ui_thread,
        })
    }
}

/// The desktop integrations an app process owns: its tray icon, global
/// shortcuts, single-instance endpoint, the power and session watch, and
/// the events they produce.
pub struct DesktopServiceState {
    event_receiver: crossbeam_channel::Receiver<PlatformEvent>,
    events: EventQueue,
    power: PowerWatcher,
    autostart: Autostart,
    tray: Option<TrayHandle>,
    _shortcuts: Option<GlobalShortcuts>,
    single_instance: Option<SingleInstanceGuard>,
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

    pub(crate) fn autostart(&self) -> Autostart {
        self.autostart.clone()
    }

    pub(crate) fn power_status(&self) -> PowerStatus {
        self.power.status()
    }

    /// The queue the app's desktop events arrive on, which window hosts
    /// also report their notifications' events to.
    pub(crate) fn events(&self) -> EventQueue {
        self.events.clone()
    }

    /// Lets later launches and the app's installer reach `process` through
    /// its single instance.
    pub(crate) fn attach(&self, process: &crate::SabineProcessHandle) {
        if let Some(instance) = &self.single_instance {
            instance.attach(process);
        }
    }

    /// Lets launches that arrive while the app ends take over once it has.
    pub(crate) fn end(&self) {
        if let Some(instance) = &self.single_instance {
            instance.end();
        }
    }
}

/// The app's login entry, which it can turn on and off while it runs.
#[derive(Clone, Debug)]
pub(crate) struct Autostart {
    entry: Option<AutostartEntry>,
    /// Development runs leave the desktop's login items alone.
    registers: bool,
}

impl Autostart {
    pub(crate) fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let Some(entry) = &self.entry else {
            return Err(
                "the app has no autostart entry; add one with SabineWindow::autostart".into(),
            );
        };
        if !self.registers {
            return Ok(());
        }
        write_autostart_entry(&AutostartEntry {
            enabled,
            ..entry.clone()
        })
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

/// What the app registers with the desktop each time it starts.
struct Registrations {
    deep_links: Vec<DeepLinkRegistration>,
    hosts: Vec<NativeMessagingHost>,
    /// The jump list's tasks, which an app without actions clears.
    #[cfg(target_os = "windows")]
    actions: Vec<sabine_service::manifest::AppAction>,
}

impl Registrations {
    /// Windows always writes the jump list, which may hold an older version's
    /// actions.
    fn is_empty(&self) -> bool {
        cfg!(not(target_os = "windows")) && self.deep_links.is_empty() && self.hosts.is_empty()
    }
}

/// Registers the app's links, native messaging hosts and Windows jump list
/// with the desktop while the app starts. A registration the desktop refuses
/// goes to the app's diagnostics and the app opens without it.
fn register_with_desktop(registrations: Registrations) {
    if registrations.is_empty() {
        return;
    }
    thread::spawn(move || {
        #[cfg(target_os = "windows")]
        if let Err(error) = platform::register_app_actions(&registrations.actions) {
            sabine_runtime::report_error(
                "desktop",
                format!("could not list the app's actions in its jump list: {error}"),
            );
        }
        for registration in &registrations.deep_links {
            if let Err(error) = register_deep_links(registration) {
                sabine_runtime::report_error(
                    "desktop",
                    format!(
                        "could not register the links of {}: {error}",
                        registration.id
                    ),
                );
            }
        }
        for host in &registrations.hosts {
            if let Err(error) = register_native_messaging_host(host) {
                sabine_runtime::report_error(
                    "desktop",
                    format!(
                        "could not register the native messaging host {}: {error}",
                        host.id
                    ),
                );
            }
        }
    });
}

/// Writes `contents` to `path` unless it already holds them, so launches
/// after the first leave the desktop's files alone.
pub(crate) fn write_if_changed(path: &Path, contents: &str) -> io::Result<()> {
    if fs::read(path).is_ok_and(|current| current == contents.as_bytes()) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
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
