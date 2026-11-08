use std::collections::BTreeSet;

use winit::event::{FingerId, Force};

use super::pen::PenState;
use crate::osr::host::input::InputKind;
use crate::osr::host::native::OsrNativeHost;

/// Fingers and the pen tip currently pressed on the page, so every press the
/// page saw ends with a release or a cancellation.
#[derive(Default)]
pub(in crate::osr::host) struct TouchState {
    fingers: BTreeSet<i32>,
    pub(super) pen: PenState,
}

#[derive(Clone, Copy)]
pub(super) enum TouchPhase {
    Pressed,
    Moved,
    Released,
    Cancelled,
}

impl TouchPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pressed => "pressed",
            Self::Moved => "moved",
            Self::Released => "released",
            Self::Cancelled => "cancelled",
        }
    }

    fn ends_touch(self) -> bool {
        matches!(self, Self::Released | Self::Cancelled)
    }
}

impl OsrNativeHost {
    pub(super) fn touch_moved(&mut self, finger: FingerId, force: Option<Force>, x: f32, y: f32) {
        let id = touch_id(finger);
        if self.touch.fingers.contains(&id) {
            self.track_touch_point(x, y);
            self.send_touch(id, x, y, TouchPhase::Moved, force, "touch");
        }
    }

    pub(super) fn touch_button(
        &mut self,
        finger: FingerId,
        force: Option<Force>,
        pressed: bool,
        x: f32,
        y: f32,
    ) {
        let id = touch_id(finger);
        self.track_touch_point(x, y);
        if pressed {
            if self.send_touch(id, x, y, TouchPhase::Pressed, force, "touch") {
                self.touch.fingers.insert(id);
            }
        } else if self.touch.fingers.remove(&id) {
            self.send_touch(id, x, y, TouchPhase::Released, force, "touch");
        }
    }

    pub(super) fn touch_left(&mut self, finger: FingerId, x: f32, y: f32) {
        let id = touch_id(finger);
        if self.touch.fingers.remove(&id) {
            self.send_touch(id, x, y, TouchPhase::Cancelled, None, "touch");
        }
    }

    pub(super) fn track_touch_point(&mut self, x: f32, y: f32) {
        self.cursor_x = x;
        self.cursor_y = y;
    }

    pub(super) fn send_touch(
        &self,
        id: i32,
        x: f32,
        y: f32,
        phase: TouchPhase,
        force: Option<Force>,
        pointer_type: &str,
    ) -> bool {
        let position = if phase.ends_touch() {
            Some(self.clamped_content_position(x, y))
        } else {
            self.content_position(x, y)
        };
        let Some((x, y)) = position else {
            return false;
        };
        let pressure = force.map_or(0.0, |force| force.normalized(None).clamp(0.0, 1.0));
        let line = format!(
            "touch\t{x:.2}\t{y:.2}\t{id}\t{}\t{pressure:.4}\t{pointer_type}\t{}\n",
            phase.as_str(),
            self.input_modifiers()
        );
        match phase {
            TouchPhase::Moved if pointer_type != "touch" => {
                self.note_input(InputKind::Pen);
                self.send_pen_move(line);
            }
            TouchPhase::Moved => {
                self.note_input(InputKind::Touch);
                self.send_touch_move(id, line);
            }
            TouchPhase::Pressed | TouchPhase::Released | TouchPhase::Cancelled => {
                self.send_control(line)
            }
        }
        true
    }
}

/// The pen's touch, which no finger shares, so Chromium never takes a palm
/// for the pen.
pub(super) const PEN_TOUCH_ID: i32 = i32::MAX;

fn touch_id(finger: FingerId) -> i32 {
    (finger.into_raw() % PEN_TOUCH_ID as usize) as i32
}
