//! The look the window's own surfaces over the page share: menus, dialogs
//! and prompts are panels in the desktop's light or dark colors, with the
//! default action in its accent color.

use sabine_platform::Appearance;

use crate::render::{DisplayList, RoundedRectCommand, TextAlign, TextCommand, TextWeight};
use crate::window::style::Color;

pub(in crate::osr::host::ui) const RADIUS: f32 = 8.0;
pub(in crate::osr::host::ui) const TEXT_SIZE: f32 = 13.0;
pub(in crate::osr::host::ui) const LINE_HEIGHT: f32 = 18.0;
pub(in crate::osr::host::ui) const BUTTON_HEIGHT: f32 = 28.0;
const BUTTON_RADIUS: f32 = 6.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::osr::host::ui) struct Rect {
    pub(in crate::osr::host::ui) x: f32,
    pub(in crate::osr::host::ui) y: f32,
    pub(in crate::osr::host::ui) width: f32,
    pub(in crate::osr::host::ui) height: f32,
}

impl Rect {
    pub(in crate::osr::host::ui) fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    pub(in crate::osr::host::ui) fn outset(self, by: f32) -> Self {
        Self {
            x: self.x - by,
            y: self.y - by,
            width: self.width + by * 2.0,
            height: self.height + by * 2.0,
        }
    }
}

/// The colors of the surfaces the window draws over the page.
pub(in crate::osr::host::ui) struct Palette {
    pub(in crate::osr::host::ui) dark: bool,
    pub(in crate::osr::host::ui) background: Color,
    pub(in crate::osr::host::ui) border: Color,
    pub(in crate::osr::host::ui) separator: Color,
    pub(in crate::osr::host::ui) highlight: Color,
    pub(in crate::osr::host::ui) text: Color,
    pub(in crate::osr::host::ui) disabled: Color,
    /// The desktop's accent color, where it has one.
    pub(in crate::osr::host::ui) accent: Option<Color>,
}

impl Palette {
    pub(in crate::osr::host::ui) fn new(appearance: Option<Appearance>) -> Self {
        let dark = appearance.is_none_or(|appearance| appearance.dark);
        let accent = appearance
            .and_then(|appearance| appearance.accent)
            .map(|[r, g, b]| Color::rgb8(r, g, b));
        if dark {
            Self {
                dark,
                background: Color::rgb8(37, 37, 41),
                border: Color::rgba8(255, 255, 255, 30),
                separator: Color::rgba8(255, 255, 255, 24),
                highlight: Color::rgba8(255, 255, 255, 22),
                text: Color::rgb8(240, 240, 242),
                disabled: Color::rgba8(240, 240, 242, 105),
                accent,
            }
        } else {
            Self {
                dark,
                background: Color::rgb8(250, 250, 251),
                border: Color::rgba8(0, 0, 0, 38),
                separator: Color::rgba8(0, 0, 0, 26),
                highlight: Color::rgba8(0, 0, 0, 18),
                text: Color::rgb8(24, 24, 27),
                disabled: Color::rgba8(24, 24, 27, 110),
                accent,
            }
        }
    }

    /// The color that marks keyboard focus and the field being typed in.
    pub(in crate::osr::host::ui) fn ring(&self) -> Color {
        self.accent.unwrap_or(self.text).opacity(0.55)
    }
}

/// Draws a panel's bordered background.
pub(in crate::osr::host::ui) fn panel(list: &mut DisplayList, rect: Rect, palette: &Palette) {
    rounded(list, rect.outset(1.0), RADIUS + 1.0, palette.border);
    rounded(list, rect, RADIUS, palette.background);
}

/// A button of a dialog or prompt as it is drawn.
pub(in crate::osr::host::ui) struct Button<'a> {
    pub(in crate::osr::host::ui) rect: Rect,
    pub(in crate::osr::host::ui) label: &'a str,
    /// The default action, filled with the accent color.
    pub(in crate::osr::host::ui) primary: bool,
    pub(in crate::osr::host::ui) hovered: bool,
    pub(in crate::osr::host::ui) pressed: bool,
    /// Whether the keyboard moved focus to it, which rings it.
    pub(in crate::osr::host::ui) focused: bool,
}

pub(in crate::osr::host::ui) fn button(list: &mut DisplayList, button: &Button, palette: &Palette) {
    let pressed = button.hovered && button.pressed;
    let (fill, label) = if button.primary {
        let (fill, label) = match palette.accent {
            Some(accent) => (accent, Color::WHITE),
            None => (palette.text, palette.background),
        };
        let opacity = match (pressed, button.hovered) {
            (true, _) => 0.8,
            (false, true) => 0.9,
            (false, false) => 1.0,
        };
        (fill.opacity(opacity), label)
    } else {
        let fill = match (pressed, button.hovered) {
            (true, _) => palette.text.opacity(if palette.dark { 0.22 } else { 0.18 }),
            (false, true) => palette.text.opacity(if palette.dark { 0.15 } else { 0.12 }),
            (false, false) => palette.highlight,
        };
        (fill, palette.text)
    };
    if button.focused {
        rounded(
            list,
            button.rect.outset(3.0),
            BUTTON_RADIUS + 3.0,
            palette.ring(),
        );
        rounded(
            list,
            button.rect.outset(1.5),
            BUTTON_RADIUS + 1.5,
            palette.background,
        );
    }
    rounded(list, button.rect, BUTTON_RADIUS, fill);
    list.push(TextCommand {
        text: button.label.into(),
        x: button.rect.x,
        y: button.rect.y + (BUTTON_HEIGHT - LINE_HEIGHT) * 0.5,
        width: button.rect.width,
        height: LINE_HEIGHT,
        size: TEXT_SIZE,
        line_height: LINE_HEIGHT,
        color: label,
        align: TextAlign::Center,
        weight: if button.primary {
            TextWeight::Semibold
        } else {
            TextWeight::Regular
        },
    });
}

pub(in crate::osr::host::ui) fn rounded(
    list: &mut DisplayList,
    rect: Rect,
    radius: f32,
    color: Color,
) {
    list.push(RoundedRectCommand {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        radius,
        color,
    });
}
