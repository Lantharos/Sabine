mod draw;
mod message;

use std::sync::Arc;

use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::ui::surface::{BUTTON_HEIGHT, LINE_HEIGHT, Rect, TEXT_SIZE};

const PADDING: f32 = 14.0;
const MAX_LINES: f32 = 3.0;
/// Wrapping at words leaves lines shorter than the room they have.
const WRAP_SLACK: f32 = 1.15;
const MIN_WIDTH: f32 = 280.0;
const MAX_WIDTH: f32 = 380.0;
const EDGE_MARGIN: f32 = 8.0;
const TOP_MARGIN: f32 = 10.0;
const BUTTON_PADDING: f32 = 14.0;
const BUTTON_GAP: f32 = 8.0;
const ROW_GAP: f32 = 12.0;

/// The question the window asks over the page when a page wants a
/// permission, laid out at the top of the window, in the context menu's
/// look.
pub(in crate::osr::host) struct PermissionPrompt {
    message: Arc<str>,
    width: f32,
    text_height: f32,
    buttons: [PromptButton; 2],
    hovered: Option<usize>,
    pressed: Option<usize>,
    /// The pointer went down on the prompt, so its release belongs to it.
    held: bool,
}

struct PromptButton {
    label: Arc<str>,
    /// From the panel's right edge to the button's left.
    offset: f32,
    width: f32,
    allows: bool,
}

impl PermissionPrompt {
    fn height(&self) -> f32 {
        PADDING * 2.0 + self.text_height + ROW_GAP + BUTTON_HEIGHT
    }

    fn button_frame(&self, panel: Rect, index: usize) -> Rect {
        let button = &self.buttons[index];
        Rect {
            x: panel.x + panel.width - button.offset,
            y: panel.y + panel.height - PADDING - BUTTON_HEIGHT,
            width: button.width,
            height: BUTTON_HEIGHT,
        }
    }

    fn button_at(&self, panel: Rect, x: f32, y: f32) -> Option<usize> {
        (0..self.buttons.len()).find(|index| self.button_frame(panel, *index).contains(x, y))
    }
}

impl OsrNativeHost {
    /// Shows the first permission request waiting for people, or hides the
    /// prompt when none waits.
    pub(in crate::osr::host) fn show_permission_prompt(&mut self) {
        let shown = self
            .permissions
            .shown()
            .map(|ask| message::message(ask, &self.config.title));
        self.permission_prompt = shown.map(|message| self.lay_out_prompt(message));
        self.request_prompt_redraw();
    }

    fn lay_out_prompt(&mut self, message: String) -> PermissionPrompt {
        let mut measure = |text: &str| match self.renderer.as_mut() {
            Some(renderer) => renderer.measure_text(text, TEXT_SIZE),
            None => text.chars().count() as f32 * TEXT_SIZE * 0.55,
        };
        let message_width = measure(&message);
        let labels = [("Allow", true), ("Don't allow", false)]
            .map(|(label, allows)| (label, allows, measure(label)));
        let room = (self.logical_width() - EDGE_MARGIN * 2.0).max(MIN_WIDTH);
        let width = (message_width.ceil() + 1.0 + PADDING * 2.0)
            .clamp(MIN_WIDTH, MAX_WIDTH)
            .min(room);
        let text_room = width - PADDING * 2.0;
        let lines = if message_width < text_room {
            1.0
        } else {
            (message_width * WRAP_SLACK / text_room)
                .ceil()
                .min(MAX_LINES)
        };
        let mut offset = PADDING;
        let buttons = labels.map(|(label, allows, text_width)| {
            let width = (text_width + BUTTON_PADDING * 2.0).ceil();
            offset += width;
            let button = PromptButton {
                label: label.into(),
                offset,
                width,
                allows,
            };
            offset += BUTTON_GAP;
            button
        });
        PermissionPrompt {
            message: message.into(),
            width,
            text_height: lines * LINE_HEIGHT,
            buttons,
            hovered: None,
            pressed: None,
            held: false,
        }
    }

    fn prompt_frame(&self) -> Option<Rect> {
        let prompt = self.permission_prompt.as_ref()?;
        Some(Rect {
            x: ((self.logical_width() - prompt.width) * 0.5).max(0.0),
            y: self.titlebar_height() + TOP_MARGIN,
            width: prompt.width,
            height: prompt.height(),
        })
    }

    /// Returns false when the pointer isn't over the prompt, so it reaches
    /// the page.
    pub(in crate::osr::host) fn prompt_pointer_moved(&mut self, x: f32, y: f32) -> bool {
        let Some(frame) = self.prompt_frame() else {
            return false;
        };
        let prompt = self
            .permission_prompt
            .as_mut()
            .expect("the prompt is shown");
        let hovered = prompt.button_at(frame, x, y);
        if hovered != prompt.hovered {
            prompt.hovered = hovered;
            self.request_prompt_redraw();
        }
        frame.contains(x, y)
    }

    pub(in crate::osr::host) fn prompt_pointer_pressed(&mut self, x: f32, y: f32) -> bool {
        let Some(frame) = self.prompt_frame() else {
            return false;
        };
        let prompt = self
            .permission_prompt
            .as_mut()
            .expect("the prompt is shown");
        let held = frame.contains(x, y);
        prompt.held = held;
        prompt.pressed = prompt.button_at(frame, x, y);
        self.request_prompt_redraw();
        held
    }

    pub(in crate::osr::host) fn prompt_pointer_released(&mut self, x: f32, y: f32) -> bool {
        let Some(frame) = self.prompt_frame() else {
            return false;
        };
        let prompt = self
            .permission_prompt
            .as_mut()
            .expect("the prompt is shown");
        let held = std::mem::take(&mut prompt.held);
        let chosen = prompt
            .pressed
            .take()
            .filter(|index| prompt.button_at(frame, x, y) == Some(*index));
        match chosen {
            Some(index) => {
                let allowed = prompt.buttons[index].allows;
                self.person_decided_permission(allowed);
            }
            None => self.request_prompt_redraw(),
        }
        held
    }

    fn request_prompt_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
