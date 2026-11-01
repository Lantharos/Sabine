use std::collections::HashMap;

use futures_util::StreamExt;
use sabine_platform::portal::{self, Options, Results};
use sabine_platform::{GlobalShortcutActivation, GlobalShortcutRegistration, PlatformEvent};
use tokio::task::JoinHandle;
use zbus::{Connection, OwnedObjectPath, OwnedVariant, Variant};

use super::super::{EventQueue, report_shortcut_failure, shortcut_key::key_code};
use super::links::ensure_shortcut_host_entry;

type PortalResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
type BoundShortcut = (String, HashMap<String, OwnedVariant>);

const GLOBAL_SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";

/// The app the portal binds shortcuts for, named by its desktop entry.
pub(in crate::desktop) struct ShortcutHost {
    app_id: String,
    name: String,
}

impl ShortcutHost {
    pub(in crate::desktop) fn new(app_id: &str, name: &str) -> Self {
        Self {
            app_id: app_id.to_string(),
            name: name.to_string(),
        }
    }
}

/// Every shortcut bound in one GlobalShortcuts portal session, so the
/// desktop asks about them together.
pub(in crate::desktop) struct GlobalShortcuts {
    session: JoinHandle<()>,
}

impl GlobalShortcuts {
    pub(in crate::desktop) fn register(
        registrations: &[GlobalShortcutRegistration],
        host: ShortcutHost,
        events: EventQueue,
    ) -> Self {
        let registrations = registrations.to_vec();
        let session = sabine_platform::async_runtime().spawn(async move {
            if let Err(error) = run_session(&registrations, &host, &events).await {
                for registration in &registrations {
                    report_shortcut_failure(&events, registration, error.to_string());
                }
            }
        });
        Self { session }
    }
}

impl Drop for GlobalShortcuts {
    fn drop(&mut self) {
        self.session.abort();
    }
}

async fn run_session(
    registrations: &[GlobalShortcutRegistration],
    host: &ShortcutHost,
    events: &EventQueue,
) -> PortalResult<()> {
    let connection = Connection::session().await?;
    ensure_shortcut_host_entry(&host.app_id, &host.name)?;
    portal::register_host_app(&connection, &host.app_id).await?;
    let shortcuts = portal::proxy(&connection, GLOBAL_SHORTCUTS).await?;
    let session = create_session(&connection, &shortcuts).await?;
    let mut activations = shortcuts.receive_signal("Activated").await?;
    let triggers = registrations.iter().map(portal_trigger).collect::<Vec<_>>();
    let requested = registrations
        .iter()
        .zip(&triggers)
        .map(|(registration, trigger)| {
            let description = registration
                .description
                .as_deref()
                .unwrap_or(&registration.action);
            (
                registration.id.as_str(),
                Options::from([
                    ("description", Variant::from(description)),
                    ("preferred_trigger", Variant::from(trigger.as_str())),
                ]),
            )
        })
        .collect::<Vec<_>>();
    let response = portal::request(&connection, |token| {
        let shortcuts = &shortcuts;
        let session = &session;
        let requested = &requested;
        async move {
            shortcuts
                .call(
                    "BindShortcuts",
                    &(
                        session,
                        requested,
                        "",
                        Options::from([("handle_token", Variant::from(token))]),
                    ),
                )
                .await
        }
    })
    .await?;
    let accepted = response
        .get("shortcuts")
        .map(|shortcuts| Vec::<BoundShortcut>::try_from(shortcuts.try_clone()?))
        .transpose()?
        .unwrap_or_default();
    let bound = registrations
        .iter()
        .filter(|registration| {
            let bound = accepted.iter().any(|(id, _)| *id == registration.id);
            if !bound {
                report_shortcut_failure(events, registration, "the desktop did not bind it");
            }
            bound
        })
        .map(|registration| (registration.id.as_str(), registration.action.as_str()))
        .collect::<HashMap<_, _>>();

    while let Some(message) = activations.next().await {
        let Ok((activated_session, id, _, options)) =
            message
                .body()
                .deserialize::<(OwnedObjectPath, String, u64, Results)>()
        else {
            continue;
        };
        if activated_session != session {
            continue;
        }
        let Some(action) = bound.get(id.as_str()) else {
            continue;
        };
        let mut activation = GlobalShortcutActivation::new(id.as_str(), *action);
        if let Some(token) = activation_token(&options) {
            activation = activation.activation_token(token);
        }
        let _ = events.send(PlatformEvent::GlobalShortcut(activation));
    }
    Ok(())
}

async fn create_session(
    connection: &Connection,
    shortcuts: &zbus::Proxy<'_>,
) -> PortalResult<OwnedObjectPath> {
    let session_token = portal::token();
    let response = portal::request(connection, |token| async move {
        shortcuts
            .call(
                "CreateSession",
                &(Options::from([
                    ("handle_token", Variant::from(token)),
                    ("session_handle_token", Variant::from(session_token)),
                ]),),
            )
            .await
    })
    .await?;
    let handle = response
        .get("session_handle")
        .ok_or("the portal did not open a shortcut session")?
        .try_clone()?;
    Ok(OwnedObjectPath::try_from(String::try_from(handle)?)?)
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

fn activation_token(options: &Results) -> Option<String> {
    let value = options.get("activation_token")?.try_clone().ok()?;
    String::try_from(value)
        .ok()
        .filter(|token| !token.trim().is_empty())
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
            description: None,
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
