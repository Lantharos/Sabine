use std::{
    collections::HashMap,
    thread::{self, JoinHandle},
};

use ashpd::desktop::{
    CreateSessionOptions,
    global_shortcuts::{BindShortcutsOptions, GlobalShortcuts as Portal, NewShortcut},
};
use futures_util::{
    StreamExt,
    future::{self, AbortHandle},
};
use sabine_platform::{GlobalShortcutActivation, GlobalShortcutRegistration, PlatformEvent};

use super::super::{EventQueue, report_shortcut_failure, shortcut_key::key_code};
use super::{UiQueue, links::ensure_shortcut_host_entry};

type PortalResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Every shortcut bound in one GlobalShortcuts portal session, so the
/// desktop asks about them together.
pub(in crate::desktop) struct GlobalShortcuts {
    abort: AbortHandle,
    thread: Option<JoinHandle<()>>,
}

impl GlobalShortcuts {
    pub(in crate::desktop) fn register(
        _queue: &UiQueue,
        registrations: &[GlobalShortcutRegistration],
        events: EventQueue,
    ) -> Self {
        let registrations = registrations.to_vec();
        let (session, abort) = future::abortable(async move {
            if let Err(error) = run_session(&registrations, &events).await {
                for registration in &registrations {
                    report_shortcut_failure(&events, registration, error.to_string());
                }
            }
        });
        let thread = thread::spawn(move || {
            let _ = pollster::block_on(session);
        });
        Self {
            abort,
            thread: Some(thread),
        }
    }
}

impl Drop for GlobalShortcuts {
    fn drop(&mut self) {
        self.abort.abort();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn run_session(
    registrations: &[GlobalShortcutRegistration],
    events: &EventQueue,
) -> PortalResult<()> {
    if let Some(registration) = registrations
        .iter()
        .find(|registration| registration.app_id.is_some())
    {
        register_host_app(registration).await?;
    }
    let portal = Portal::new().await?;
    let session = portal
        .create_session(CreateSessionOptions::default())
        .await?;
    let mut activations = portal.receive_activated().await?;
    let triggers = registrations.iter().map(portal_trigger).collect::<Vec<_>>();
    let shortcuts = registrations
        .iter()
        .zip(&triggers)
        .map(|(registration, trigger)| {
            let description = registration
                .description
                .as_deref()
                .unwrap_or(&registration.action);
            NewShortcut::new(registration.id.as_str(), description)
                .preferred_trigger(Some(trigger.as_str()))
        })
        .collect::<Vec<_>>();
    let response = portal
        .bind_shortcuts(&session, &shortcuts, None, BindShortcutsOptions::default())
        .await?
        .response()?;
    let bound = registrations
        .iter()
        .filter(|registration| {
            let bound = response
                .shortcuts()
                .iter()
                .any(|shortcut| shortcut.id() == registration.id);
            if !bound {
                report_shortcut_failure(events, registration, "the desktop did not bind it");
            }
            bound
        })
        .map(|registration| (registration.id.as_str(), registration.action.as_str()))
        .collect::<HashMap<_, _>>();

    while let Some(event) = activations.next().await {
        let Some(action) = bound.get(event.shortcut_id()) else {
            continue;
        };
        let mut activation = GlobalShortcutActivation::new(event.shortcut_id(), *action);
        if let Some(token) = activation_token(event.options()) {
            activation = activation.activation_token(token);
        }
        let _ = events.send(PlatformEvent::GlobalShortcut(activation));
    }
    Ok(())
}

/// The XDG shortcut trigger, such as `CTRL+SHIFT+space`.
fn portal_trigger(registration: &GlobalShortcutRegistration) -> String {
    let modifiers = registration.shortcut.modifiers;
    let mut parts = [
        (modifiers.ctrl, "CTRL"),
        (modifiers.alt, "ALT"),
        (modifiers.shift, "SHIFT"),
        (modifiers.meta, "LOGO"),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .map(|(_, name)| name.to_string())
    .collect::<Vec<_>>();
    parts.push(keysym(&key_code(&registration.shortcut.key)));
    parts.join("+")
}

/// The XKB keysym for a key code name. Other names are passed on as keysyms.
fn keysym(code: &str) -> String {
    if let Some(letter) = code.strip_prefix("Key") {
        return letter.to_ascii_lowercase();
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return digit.to_string();
    }
    if let Some(digit) = code.strip_prefix("Numpad").filter(|rest| rest.len() == 1) {
        return format!("KP_{digit}");
    }
    let named = match code {
        "Space" => "space",
        "Enter" => "Return",
        "Backspace" => "BackSpace",
        "PageUp" => "Page_Up",
        "PageDown" => "Page_Down",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "PrintScreen" => "Print",
        "CapsLock" => "Caps_Lock",
        "NumLock" => "Num_Lock",
        "ScrollLock" => "Scroll_Lock",
        "Minus" => "minus",
        "Equal" => "equal",
        "BracketLeft" => "bracketleft",
        "BracketRight" => "bracketright",
        "Backslash" => "backslash",
        "Semicolon" => "semicolon",
        "Quote" => "apostrophe",
        "Comma" => "comma",
        "Period" => "period",
        "Slash" => "slash",
        "Backquote" => "grave",
        "NumpadAdd" => "KP_Add",
        "NumpadSubtract" => "KP_Subtract",
        "NumpadMultiply" => "KP_Multiply",
        "NumpadDivide" => "KP_Divide",
        "NumpadDecimal" => "KP_Decimal",
        "NumpadEnter" => "KP_Enter",
        "NumpadEqual" => "KP_Equal",
        "AudioVolumeUp" => "XF86AudioRaiseVolume",
        "AudioVolumeDown" => "XF86AudioLowerVolume",
        "AudioVolumeMute" => "XF86AudioMute",
        "MediaPlayPause" | "MediaPlay" => "XF86AudioPlay",
        "MediaPause" => "XF86AudioPause",
        "MediaStop" => "XF86AudioStop",
        "MediaTrackNext" => "XF86AudioNext",
        "MediaTrackPrevious" => "XF86AudioPrev",
        other => other,
    };
    named.to_string()
}

fn activation_token(options: &HashMap<String, ashpd::zvariant::OwnedValue>) -> Option<String> {
    let value = options.get("activation_token")?.try_clone().ok()?;
    String::try_from(value)
        .ok()
        .filter(|token| !token.trim().is_empty())
}

/// Tells the portal which desktop entry this unsandboxed process belongs to.
async fn register_host_app(registration: &GlobalShortcutRegistration) -> PortalResult<()> {
    let Some(app_id) = registration.app_id.as_deref() else {
        return Ok(());
    };
    if let Some(command) = registration.desktop_command.as_deref() {
        let name = registration
            .app_name
            .as_deref()
            .or(registration.description.as_deref())
            .unwrap_or(app_id);
        ensure_shortcut_host_entry(app_id, name, command)?;
    }
    match ashpd::register_host_app(ashpd::AppID::try_from(app_id)?).await {
        Err(error) if !already_registered(&error) => Err(Box::new(error)),
        _ => Ok(()),
    }
}

fn already_registered(error: &ashpd::Error) -> bool {
    let message = error.to_string();
    message.contains("already associated") || message.contains("already registered")
}

#[cfg(test)]
mod tests {
    use sabine_platform::{GlobalShortcutRegistration, Shortcut, ShortcutModifiers};

    use super::portal_trigger;

    fn registration(key: &str) -> GlobalShortcutRegistration {
        GlobalShortcutRegistration {
            id: "toggle".into(),
            shortcut: Shortcut {
                modifiers: ShortcutModifiers {
                    ctrl: true,
                    shift: true,
                    ..ShortcutModifiers::default()
                },
                key: key.into(),
            },
            action: "toggle".into(),
            app_id: None,
            app_name: None,
            description: None,
            desktop_command: None,
        }
    }

    #[test]
    fn triggers_use_keysym_names() {
        assert_eq!(portal_trigger(&registration("Space")), "CTRL+SHIFT+space");
        assert_eq!(portal_trigger(&registration("K")), "CTRL+SHIFT+k");
        assert_eq!(
            portal_trigger(&registration("PageDown")),
            "CTRL+SHIFT+Page_Down"
        );
        assert_eq!(
            portal_trigger(&registration("XF86Calculator")),
            "CTRL+SHIFT+XF86Calculator"
        );
    }
}
