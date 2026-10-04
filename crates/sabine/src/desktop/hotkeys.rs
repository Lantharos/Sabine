use std::{cell::RefCell, collections::HashMap};

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use sabine_platform::{
    GlobalShortcutActivation, GlobalShortcutRegistration, PlatformEvent, Shortcut,
};

use super::{EventQueue, platform::UiQueue, report_shortcut_failure, shortcut_key::key_code};

thread_local! {
    static MANAGER: RefCell<Option<GlobalHotKeyManager>> = const { RefCell::new(None) };
}

/// System-wide hotkeys, registered on the thread that runs the desktop's
/// event loop. A hotkey another app holds is reported and skipped.
pub(super) struct GlobalShortcuts {
    queue: UiQueue,
}

impl GlobalShortcuts {
    pub(super) fn register(
        queue: &UiQueue,
        registrations: &[GlobalShortcutRegistration],
        events: EventQueue,
    ) -> Self {
        let mut hotkeys = Vec::new();
        for registration in registrations {
            match hotkey(&registration.shortcut) {
                Ok(hotkey) => hotkeys.push((registration.clone(), hotkey)),
                Err(message) => report_shortcut_failure(&events, registration, message),
            }
        }
        let actions = hotkeys
            .iter()
            .map(|(registration, hotkey)| {
                (
                    hotkey.id(),
                    (registration.id.clone(), registration.action.clone()),
                )
            })
            .collect::<HashMap<_, _>>();
        let activations = events.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state == HotKeyState::Pressed
                && let Some((id, action)) = actions.get(&event.id())
            {
                let _ = activations.send(PlatformEvent::GlobalShortcut(
                    GlobalShortcutActivation::new(id.clone(), action.clone()),
                ));
            }
        }));
        queue.run(move || {
            let manager = match GlobalHotKeyManager::new() {
                Ok(manager) => manager,
                Err(error) => {
                    for (registration, _) in &hotkeys {
                        report_shortcut_failure(&events, registration, error.to_string());
                    }
                    return;
                }
            };
            for (registration, hotkey) in &hotkeys {
                if let Err(error) = manager.register(*hotkey) {
                    report_shortcut_failure(&events, registration, error.to_string());
                }
            }
            MANAGER.with(|current| *current.borrow_mut() = Some(manager));
        });
        Self {
            queue: queue.clone(),
        }
    }
}

impl Drop for GlobalShortcuts {
    fn drop(&mut self) {
        self.queue.run(|| {
            MANAGER.with(|manager| manager.borrow_mut().take());
        });
    }
}

fn hotkey(shortcut: &Shortcut) -> Result<HotKey, String> {
    let code = key_code(&shortcut.key).parse::<Code>().map_err(|_| {
        format!(
            "{} is not a key that can be a global shortcut",
            shortcut.key
        )
    })?;
    let mut modifiers = Modifiers::empty();
    modifiers.set(Modifiers::CONTROL, shortcut.modifiers.ctrl);
    modifiers.set(Modifiers::ALT, shortcut.modifiers.alt);
    modifiers.set(Modifiers::SHIFT, shortcut.modifiers.shift);
    modifiers.set(Modifiers::SUPER, shortcut.modifiers.meta);
    Ok(HotKey::new(Some(modifiers), code))
}
