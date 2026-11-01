mod ime;
#[cfg(target_os = "linux")]
mod panel;
mod shortcuts;
#[cfg(windows)]
mod touch_keyboard;

#[cfg(target_os = "linux")]
pub(in crate::osr::host) use panel::InputPanelState;
pub(in crate::osr::host) use shortcuts::ShortcutInhibition;
#[cfg(windows)]
pub(in crate::osr::host) use touch_keyboard::TouchKeyboard;

use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{Key, NamedKey},
    platform::scancode::PhysicalKeyExtScancode,
};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{
    EVENTFLAG_ALT_DOWN, EVENTFLAG_COMMAND_DOWN, EVENTFLAG_CONTROL_DOWN, EVENTFLAG_IS_REPEAT,
    EVENTFLAG_SHIFT_DOWN,
};
use crate::osr::protocol::encode_component;

const XKB_KEYCODE_OFFSET: u32 = 8;

impl OsrNativeHost {
    pub(in crate::osr::host) fn send_key_event(&self, event: &KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        let text = if pressed {
            event
                .text
                .as_deref()
                .filter(|text| should_send_char_text(text))
                .unwrap_or("")
        } else {
            ""
        };
        let modifier = modifier_flag(&event.logical_key);
        let modifiers = if pressed {
            self.input_modifiers() | modifier
        } else {
            self.input_modifiers() & !modifier
        };
        self.send_key(
            pressed,
            &key_name(event),
            text,
            modifiers,
            event.repeat,
            native_key_code(event),
        );
    }

    #[cfg(windows)]
    pub(in crate::osr::host) fn send_system_key(&self, key: sabine_platform::SystemKey) {
        let held = [
            (key.modifiers.shift, EVENTFLAG_SHIFT_DOWN),
            (key.modifiers.ctrl, EVENTFLAG_CONTROL_DOWN),
            (key.modifiers.alt, EVENTFLAG_ALT_DOWN),
            (key.modifiers.meta, EVENTFLAG_COMMAND_DOWN),
        ];
        let modifiers = held
            .into_iter()
            .filter(|(down, _)| *down)
            .fold(self.mouse_modifiers(), |modifiers, (_, flag)| {
                modifiers | flag
            });
        self.send_key(
            key.pressed,
            key.key,
            "",
            modifiers,
            key.repeat,
            key.scan_code,
        );
    }

    fn send_key(
        &self,
        pressed: bool,
        name: &str,
        text: &str,
        modifiers: u32,
        repeat: bool,
        native_key_code: u32,
    ) {
        self.send_control(format!(
            "key\t{}\t{}\t{}\t{}\t{}\t{}\n",
            i32::from(pressed),
            encode_component(name),
            encode_component(text),
            modifiers | if repeat { EVENTFLAG_IS_REPEAT } else { 0 },
            i32::from(repeat),
            native_key_code
        ));
    }
}

fn key_name(event: &KeyEvent) -> String {
    match event.logical_key.as_ref() {
        Key::Character(value) if !value.is_empty() => value.to_string(),
        Key::Named(named) => named.to_string(),
        _ => match &event.physical_key {
            winit::keyboard::PhysicalKey::Code(code) => format!("{code:?}"),
            _ => "Unidentified".to_string(),
        },
    }
}

fn should_send_char_text(text: &str) -> bool {
    text == "\r" || !text.chars().any(char::is_control)
}

fn modifier_flag(key: &Key) -> u32 {
    match key {
        Key::Named(NamedKey::Shift) => EVENTFLAG_SHIFT_DOWN,
        Key::Named(NamedKey::Control) => EVENTFLAG_CONTROL_DOWN,
        Key::Named(NamedKey::Alt) => EVENTFLAG_ALT_DOWN,
        Key::Named(NamedKey::Meta) => EVENTFLAG_COMMAND_DOWN,
        _ => 0,
    }
}

fn native_key_code(event: &KeyEvent) -> u32 {
    let Some(scancode) = event.physical_key.to_scancode() else {
        return 0;
    };
    if cfg!(target_os = "linux") {
        scancode + XKB_KEYCODE_OFFSET
    } else {
        scancode
    }
}
