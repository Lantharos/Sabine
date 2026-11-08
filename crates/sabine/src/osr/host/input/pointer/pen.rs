use winit::event::{MouseButton, TabletToolButton, TabletToolData, TabletToolKind, TabletToolTilt};

use super::touch::{PEN_TOUCH_ID, TouchPhase};
use crate::osr::host::native::OsrNativeHost;

const BARREL_BUTTON: u8 = 1;
const SECOND_BARREL_BUTTON: u8 = 2;
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
const BTN_MIDDLE: u16 = 0x112;
const BTN_SIDE: u16 = 0x113;
const BTN_EXTRA: u16 = 0x114;

/// The mouse button a pen button acts as. A compositor that maps a side
/// button to a click, as GNOME's and Luft's pen settings do, sends the mouse
/// button's own code instead of the barrel button's.
pub(super) fn pen_mouse_button(button: TabletToolButton) -> Option<MouseButton> {
    match button {
        TabletToolButton::Other(BTN_LEFT) => Some(MouseButton::Left),
        TabletToolButton::Other(BTN_RIGHT) => Some(MouseButton::Right),
        TabletToolButton::Other(BTN_MIDDLE) => Some(MouseButton::Middle),
        TabletToolButton::Other(BTN_SIDE) => Some(MouseButton::Back),
        TabletToolButton::Other(BTN_EXTRA) => Some(MouseButton::Forward),
        other => other.into(),
    }
}

/// What the page last heard about the tablet tool near the screen.
#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct PenState {
    eraser: bool,
    near: bool,
    pub(super) contact: bool,
    buttons: u8,
    tilt: Option<TabletToolTilt>,
}

impl PenState {
    fn line(&self) -> String {
        let tool = if self.eraser { "eraser" } else { "pen" };
        let tilt =
            |axis: Option<i8>| axis.map_or("null".to_string(), |degrees| degrees.to_string());
        format!(
            "pen\t{{\"tool\":\"{tool}\",\"near\":{},\"contact\":{},\"buttons\":{},\"tiltX\":{},\"tiltY\":{}}}\n",
            self.near,
            self.contact,
            self.buttons,
            tilt(self.tilt.map(|tilt| tilt.x)),
            tilt(self.tilt.map(|tilt| tilt.y)),
        )
    }
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn pen_entered(&mut self, kind: TabletToolKind) {
        self.update_pen(|pen| {
            pen.eraser = kind == TabletToolKind::Eraser;
            pen.near = true;
        });
    }

    pub(super) fn pen_moved(&mut self, kind: TabletToolKind, data: TabletToolData, x: f32, y: f32) {
        let tilt = data.clone().tilt();
        self.update_pen(|pen| {
            pen.eraser = kind == TabletToolKind::Eraser;
            pen.near = true;
            pen.tilt = tilt;
        });
        if self.touch.pen.contact {
            self.track_touch_point(x, y);
            let pointer = pointer_type(kind);
            self.send_touch(PEN_TOUCH_ID, x, y, TouchPhase::Moved, data.force, pointer);
        } else {
            self.mouse_moved(x, y);
        }
    }

    pub(super) fn pen_button(
        &mut self,
        kind: TabletToolKind,
        data: TabletToolData,
        pressed: bool,
        x: f32,
        y: f32,
    ) {
        self.track_touch_point(x, y);
        let pointer = pointer_type(kind);
        if pressed {
            if self.content_position(x, y).is_some() {
                self.update_pen(|pen| pen.contact = true);
                self.send_touch(PEN_TOUCH_ID, x, y, TouchPhase::Pressed, data.force, pointer);
            }
        } else if self.touch.pen.contact {
            self.update_pen(|pen| pen.contact = false);
            self.send_touch(
                PEN_TOUCH_ID,
                x,
                y,
                TouchPhase::Released,
                data.force,
                pointer,
            );
        }
    }

    /// Notes a barrel button, and returns whether it also acts as a mouse
    /// button, which it does unless it went down with the tip on the screen.
    pub(super) fn pen_barrel_button(&mut self, button: TabletToolButton, pressed: bool) -> bool {
        let bit = match button {
            TabletToolButton::Barrel => BARREL_BUTTON,
            TabletToolButton::Other(1) => SECOND_BARREL_BUTTON,
            _ => 0,
        };
        self.update_pen(|pen| {
            if pressed {
                pen.buttons |= bit;
            } else {
                pen.buttons &= !bit;
            }
        });
        !(pressed && self.touch.pen.contact)
    }

    pub(super) fn pen_left(&mut self, kind: TabletToolKind, x: f32, y: f32) {
        let contact = self.touch.pen.contact;
        self.update_pen(|pen| {
            *pen = PenState {
                eraser: pen.eraser,
                ..PenState::default()
            }
        });
        if contact {
            let pointer = pointer_type(kind);
            self.send_touch(PEN_TOUCH_ID, x, y, TouchPhase::Cancelled, None, pointer);
        } else {
            self.mouse_left(x, y);
        }
    }

    fn update_pen(&mut self, change: impl FnOnce(&mut PenState)) {
        let previous = self.touch.pen;
        change(&mut self.touch.pen);
        if self.touch.pen != previous {
            self.send_control(self.touch.pen.line());
        }
    }
}

fn pointer_type(kind: TabletToolKind) -> &'static str {
    match kind {
        TabletToolKind::Eraser => "eraser",
        TabletToolKind::Pen
        | TabletToolKind::Brush
        | TabletToolKind::Pencil
        | TabletToolKind::Airbrush => "pen",
        _ => "unknown",
    }
}
