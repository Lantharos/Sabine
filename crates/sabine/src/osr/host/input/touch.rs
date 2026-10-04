use std::collections::BTreeSet;

use winit::event::{FingerId, Force, TabletToolButton, TabletToolData, TabletToolKind};

use crate::osr::host::native::OsrNativeHost;

/// Fingers and the pen tip currently pressed on the page, so every press the
/// page saw ends with a release or a cancellation.
#[derive(Default)]
pub(in crate::osr::host) struct TouchState {
    fingers: BTreeSet<i32>,
    pen_contact: bool,
}

#[derive(Clone, Copy)]
enum TouchPhase {
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

const PEN_TOUCH_ID: i32 = 0;

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

    pub(super) fn pen_moved(&mut self, kind: TabletToolKind, data: TabletToolData, x: f32, y: f32) {
        if self.touch.pen_contact {
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
        button: TabletToolButton,
        data: TabletToolData,
        pressed: bool,
        x: f32,
        y: f32,
    ) {
        if button != TabletToolButton::Contact {
            return;
        }
        self.track_touch_point(x, y);
        let pointer = pointer_type(kind);
        if pressed {
            self.touch.pen_contact =
                self.send_touch(PEN_TOUCH_ID, x, y, TouchPhase::Pressed, data.force, pointer);
        } else if std::mem::take(&mut self.touch.pen_contact) {
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

    pub(super) fn pen_left(&mut self, kind: TabletToolKind, x: f32, y: f32) {
        if std::mem::take(&mut self.touch.pen_contact) {
            let pointer = pointer_type(kind);
            self.send_touch(PEN_TOUCH_ID, x, y, TouchPhase::Cancelled, None, pointer);
        } else {
            self.mouse_left(x, y);
        }
    }

    fn track_touch_point(&mut self, x: f32, y: f32) {
        self.cursor_x = x;
        self.cursor_y = y;
    }

    fn send_touch(
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
        self.send_control(format!(
            "touch\t{x:.2}\t{y:.2}\t{id}\t{}\t{pressure:.4}\t{pointer_type}\t{}\n",
            phase.as_str(),
            self.input_modifiers()
        ));
        true
    }
}

fn touch_id(finger: FingerId) -> i32 {
    (finger.into_raw() % i32::MAX as usize) as i32
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
