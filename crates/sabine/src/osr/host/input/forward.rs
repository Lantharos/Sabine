use std::time::{Duration, Instant};

use winit::{
    event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta},
    keyboard::{Key, NamedKey},
    platform::scancode::PhysicalKeyExtScancode,
};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{
    ClickMemory, EVENTFLAG_ALT_DOWN, EVENTFLAG_COMMAND_DOWN, EVENTFLAG_CONTROL_DOWN,
    EVENTFLAG_IS_REPEAT, EVENTFLAG_LEFT_MOUSE_BUTTON, EVENTFLAG_MIDDLE_MOUSE_BUTTON,
    EVENTFLAG_PRECISION_SCROLLING_DELTA, EVENTFLAG_RIGHT_MOUSE_BUTTON, EVENTFLAG_SHIFT_DOWN,
};
use crate::osr::protocol::encode_component;

const XKB_KEYCODE_OFFSET: u32 = 8;
const WHEEL_DELTA_PER_LINE: f64 = 120.0;
const PINCH_WHEEL_DELTA: f64 = 100.0;

/// Fractions of a wheel step not yet sent, so slow trackpad motion adds up
/// instead of being rounded away.
#[derive(Default)]
pub(in crate::osr::host) struct WheelRemainder {
    scroll: AxisRemainder,
    pinch: AxisRemainder,
}

#[derive(Default)]
struct AxisRemainder {
    x: f64,
    y: f64,
}

impl AxisRemainder {
    fn take(&mut self, dx: f64, dy: f64) -> (i32, i32) {
        (whole_steps(&mut self.x, dx), whole_steps(&mut self.y, dy))
    }
}

fn whole_steps(remainder: &mut f64, delta: f64) -> i32 {
    if delta == 0.0 {
        return 0;
    }
    if remainder.signum() != delta.signum() {
        *remainder = 0.0;
    }
    let total = *remainder + delta;
    let whole = total.trunc();
    *remainder = total - whole;
    whole as i32
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn forward_mouse_move(&self, leave: bool) {
        if let Some((x, y)) = self.content_position(self.cursor_x, self.cursor_y) {
            self.send_mouse_motion(format!(
                "mouse_move\t{:.2}\t{:.2}\t{}\t{}\n",
                x,
                y,
                self.input_modifiers(),
                i32::from(leave)
            ));
        }
    }

    pub(in crate::osr::host) fn forward_mouse_click(
        &self,
        button: Option<MouseButton>,
        up: bool,
        click_count: i32,
    ) {
        let Some((x, y)) = self.content_position(self.cursor_x, self.cursor_y) else {
            return;
        };
        let Some(button) = cef_mouse_button(button) else {
            return;
        };
        self.send_control(format!(
            "mouse_click\t{:.2}\t{:.2}\t{}\t{}\t{}\t{}\n",
            x,
            y,
            button,
            self.input_modifiers(),
            i32::from(up),
            click_count.max(1)
        ));
    }

    pub(in crate::osr::host) fn forward_navigation_button(&self, button: Option<MouseButton>) {
        let Some((x, y)) = self.content_position(self.cursor_x, self.cursor_y) else {
            return;
        };
        let button = match button {
            Some(MouseButton::Back) => 3,
            Some(MouseButton::Forward) => 4,
            _ => return,
        };
        self.send_control(format!(
            "mouse_navigation\t{:.2}\t{:.2}\t{}\t{}\n",
            x,
            y,
            button,
            self.input_modifiers()
        ));
    }

    pub(in crate::osr::host) fn forward_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let (dx, dy, precision) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (
                f64::from(x) * WHEEL_DELTA_PER_LINE,
                f64::from(y) * WHEEL_DELTA_PER_LINE,
                0,
            ),
            MouseScrollDelta::PixelDelta(position) => {
                (position.x, position.y, EVENTFLAG_PRECISION_SCROLLING_DELTA)
            }
            _ => return,
        };
        let modifiers = self.input_modifiers() | precision;
        let (dx, dy) = self.wheel_remainder.scroll.take(dx, dy);
        self.send_wheel(dx, dy, modifiers);
    }

    /// Pinches reach pages as Ctrl+wheel, as Chromium reports touchpad pinch.
    pub(in crate::osr::host) fn forward_pinch(&mut self, delta: f64) {
        let scale = 1.0 + delta;
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        let modifiers =
            self.input_modifiers() | EVENTFLAG_CONTROL_DOWN | EVENTFLAG_PRECISION_SCROLLING_DELTA;
        let (_, dy) = self
            .wheel_remainder
            .pinch
            .take(0.0, PINCH_WHEEL_DELTA * scale.ln());
        self.send_wheel(0, dy, modifiers);
    }

    fn send_wheel(&self, dx: i32, dy: i32, modifiers: u32) {
        if dx == 0 && dy == 0 {
            return;
        }
        let Some((x, y)) = self.content_position(self.cursor_x, self.cursor_y) else {
            return;
        };
        self.send_control(format!(
            "mouse_wheel\t{x:.2}\t{y:.2}\t{dx}\t{dy}\t{modifiers}\n"
        ));
    }

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

    pub(in crate::osr::host) fn input_modifiers(&self) -> u32 {
        let mut modifiers = 0;
        if self.modifiers.shift_key() {
            modifiers |= EVENTFLAG_SHIFT_DOWN;
        }
        if self.modifiers.control_key() {
            modifiers |= EVENTFLAG_CONTROL_DOWN;
        }
        if self.modifiers.alt_key() {
            modifiers |= EVENTFLAG_ALT_DOWN;
        }
        if self.modifiers.meta_key() {
            modifiers |= EVENTFLAG_COMMAND_DOWN;
        }
        modifiers | self.mouse_modifiers()
    }

    fn mouse_modifiers(&self) -> u32 {
        let mut modifiers = 0;
        if self.mouse.left {
            modifiers |= EVENTFLAG_LEFT_MOUSE_BUTTON;
        }
        if self.mouse.middle {
            modifiers |= EVENTFLAG_MIDDLE_MOUSE_BUTTON;
        }
        if self.mouse.right {
            modifiers |= EVENTFLAG_RIGHT_MOUSE_BUTTON;
        }
        modifiers
    }

    pub(in crate::osr::host) fn set_mouse_button(
        &mut self,
        button: Option<MouseButton>,
        pressed: bool,
    ) {
        match button {
            Some(MouseButton::Left) => self.mouse.left = pressed,
            Some(MouseButton::Middle) => self.mouse.middle = pressed,
            Some(MouseButton::Right) => self.mouse.right = pressed,
            _ => {}
        }
    }

    pub(in crate::osr::host) fn mouse_button_pressed(&self, button: Option<MouseButton>) -> bool {
        match button {
            Some(MouseButton::Left) => self.mouse.left,
            Some(MouseButton::Middle) => self.mouse.middle,
            Some(MouseButton::Right) => self.mouse.right,
            _ => false,
        }
    }

    pub(in crate::osr::host) fn next_click_count(&mut self, button: Option<MouseButton>) -> i32 {
        let Some(button) = button else {
            return 1;
        };
        let now = Instant::now();
        let count = self
            .last_click
            .filter(|last| {
                last.button == button
                    && now.duration_since(last.at) <= Duration::from_millis(500)
                    && (last.x - self.cursor_x).abs() <= 4.0
                    && (last.y - self.cursor_y).abs() <= 4.0
            })
            .map(|last| (last.count + 1).min(3))
            .unwrap_or(1);
        self.last_click = Some(ClickMemory {
            button,
            x: self.cursor_x,
            y: self.cursor_y,
            at: now,
            count,
        });
        count
    }
}

fn cef_mouse_button(button: Option<MouseButton>) -> Option<&'static str> {
    match button {
        Some(MouseButton::Left) => Some("left"),
        Some(MouseButton::Middle) => Some("middle"),
        Some(MouseButton::Right) => Some("right"),
        _ => None,
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
