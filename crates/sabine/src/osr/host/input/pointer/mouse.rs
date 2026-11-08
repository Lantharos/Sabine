use std::time::{Duration, Instant};

use winit::event::{MouseButton, MouseScrollDelta};

use crate::osr::control::Wheel;
use crate::osr::host::input::InputKind;
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::{
    ClickMemory, EVENTFLAG_ALT_DOWN, EVENTFLAG_COMMAND_DOWN, EVENTFLAG_CONTROL_DOWN,
    EVENTFLAG_LEFT_MOUSE_BUTTON, EVENTFLAG_MIDDLE_MOUSE_BUTTON,
    EVENTFLAG_PRECISION_SCROLLING_DELTA, EVENTFLAG_RIGHT_MOUSE_BUTTON, EVENTFLAG_SHIFT_DOWN,
    MouseButtons,
};

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
    pub(in crate::osr::host) fn forward_mouse_move(&self) {
        if let Some((x, y)) = self.content_position(self.cursor_x, self.cursor_y) {
            self.note_input(InputKind::Pointer);
            self.send_mouse_motion(format!(
                "mouse_move\t{x:.2}\t{y:.2}\t{}\t0\n",
                self.input_modifiers()
            ));
        }
    }

    /// Tells the page the pointer left it, which no later motion may replace,
    /// or the page would keep showing what was under the pointer as hovered.
    pub(in crate::osr::host) fn forward_mouse_leave(&self) {
        let (x, y) = self.clamped_content_position(self.cursor_x, self.cursor_y);
        self.send_control(format!(
            "mouse_move\t{x:.2}\t{y:.2}\t{}\t1\n",
            self.input_modifiers()
        ));
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
                let position = position.to_logical::<f64>(self.scale());
                (position.x, position.y, EVENTFLAG_PRECISION_SCROLLING_DELTA)
            }
            _ => return,
        };
        let modifiers = self.input_modifiers() | precision;
        let (dx, dy) = self.wheel_remainder.scroll.take(dx, dy);
        self.wheel_at_pointer(dx, dy, modifiers);
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
        self.wheel_at_pointer(0, dy, modifiers);
    }

    fn wheel_at_pointer(&self, dx: i32, dy: i32, modifiers: u32) {
        if dx == 0 && dy == 0 {
            return;
        }
        let Some((x, y)) = self.content_position(self.cursor_x, self.cursor_y) else {
            return;
        };
        self.send_wheel(Wheel {
            x,
            y,
            dx,
            dy,
            modifiers,
        });
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

    pub(in crate::osr::host) fn mouse_modifiers(&self) -> u32 {
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

    /// Chromium keeps a pressed button's pointer capture until the button goes up, which a window
    /// that loses focus while the button is down never hears. A drag the page started ends
    /// through the drag instead.
    pub(in crate::osr::host) fn lose_pointer_capture(&mut self) {
        if self.drag.page_operations().is_some()
            || !(self.mouse.left || self.mouse.middle || self.mouse.right)
        {
            return;
        }
        self.mouse = MouseButtons::default();
        self.send_control("capture_lost\n");
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
