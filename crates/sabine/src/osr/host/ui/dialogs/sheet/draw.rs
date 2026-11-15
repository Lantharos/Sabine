use std::sync::Arc;

use super::layout::{
    BUTTON_HEIGHT, FIELD_INSET, MESSAGE_LINE, MESSAGE_SIZE, Rect, SheetLayout, TITLE_LINE,
    TITLE_SIZE,
};
use super::{OpenSheet, Target};
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::ui::context_menu::draw::{MenuPalette, RADIUS};
use crate::render::{
    DisplayList, RectCommand, RoundedRectCommand, TextAlign, TextCommand, TextWeight,
};
use crate::window::style::Color;

const CONTROL_RADIUS: f32 = 6.0;
const CARET_INSET: f32 = 7.0;

impl OsrNativeHost {
    pub(in crate::osr::host) fn draw_sheet(&self, list: &mut DisplayList, width: f32, height: f32) {
        let Some(open) = &self.sheets.open else {
            return;
        };
        let Some(layout) = &open.layout else {
            return;
        };
        let appearance = self.appearance();
        let dark = appearance.is_none_or(|appearance| appearance.dark);
        let palette = SheetPalette::new(
            MenuPalette::new(dark),
            appearance.and_then(|appearance| appearance.accent),
            dark,
        );
        let top = self.titlebar_height();
        list.push(RectCommand {
            x: 0.0,
            y: top,
            width,
            height: height - top,
            color: palette.scrim,
        });
        rounded(
            list,
            outset(layout.panel, 1.0),
            RADIUS + 1.0,
            palette.menu.border,
        );
        rounded(list, layout.panel, RADIUS, palette.menu.background);
        if let Some(title) = layout.title {
            list.push(text(
                &open.dialog.title,
                title,
                TITLE_SIZE,
                TITLE_LINE,
                palette.menu.text,
                TextWeight::Semibold,
            ));
        }
        if let Some(message) = layout.message {
            list.push(text(
                &open.dialog.message,
                message,
                MESSAGE_SIZE,
                MESSAGE_LINE,
                palette.secondary,
                TextWeight::Regular,
            ));
        }
        draw_field(list, open, layout, &palette);
        for (index, rect) in layout.buttons.iter().copied().enumerate() {
            draw_button(list, open, index, rect, &palette);
        }
    }
}

fn draw_field(
    list: &mut DisplayList,
    open: &OpenSheet,
    layout: &SheetLayout,
    palette: &SheetPalette,
) {
    let (Some(field), Some(rect)) = (&open.field, layout.field) else {
        return;
    };
    let focused = open.focus == Target::Field;
    let border = if focused {
        palette.ring
    } else {
        palette.menu.border
    };
    rounded(list, outset(rect, 1.0), CONTROL_RADIUS + 1.0, border);
    rounded(list, rect, CONTROL_RADIUS, palette.field);
    let line = Rect {
        x: rect.x + FIELD_INSET,
        y: rect.y + (rect.height - MESSAGE_LINE) * 0.5,
        width: rect.width - FIELD_INSET * 2.0,
        height: MESSAGE_LINE,
    };
    let shown = field.shown();
    if shown.is_empty() {
        let placeholder = open
            .dialog
            .input
            .as_ref()
            .map_or("", |input| input.placeholder.as_str());
        list.push(text(
            placeholder,
            line,
            MESSAGE_SIZE,
            MESSAGE_LINE,
            palette.menu.disabled,
            TextWeight::Regular,
        ));
    }
    let selection = field.selection();
    if focused && !selection.is_empty() {
        let (start, end) = (
            layout.field_x(selection.start),
            layout.field_x(selection.end),
        );
        list.push(RectCommand {
            x: start,
            y: rect.y + CARET_INSET - 1.0,
            width: end - start,
            height: rect.height - (CARET_INSET - 1.0) * 2.0,
            color: palette.selection,
        });
    }
    list.push(text(
        &shown[layout.view.clone()],
        Rect {
            width: line.width + 2.0,
            ..line
        },
        MESSAGE_SIZE,
        MESSAGE_LINE,
        palette.menu.text,
        TextWeight::Regular,
    ));
    if !field.preedit.is_empty() {
        let (start, end) = (
            layout.field_x(field.caret),
            layout.field_x(field.caret + field.preedit.len()),
        );
        list.push(RectCommand {
            x: start,
            y: line.y + line.height - 2.0,
            width: end - start,
            height: 1.0,
            color: palette.menu.text,
        });
    }
    if focused && selection.is_empty() {
        list.push(RectCommand {
            x: layout.field_x(field.caret + field.preedit.len()).round(),
            y: rect.y + CARET_INSET,
            width: 1.0,
            height: rect.height - CARET_INSET * 2.0,
            color: palette.menu.text,
        });
    }
}

fn draw_button(
    list: &mut DisplayList,
    open: &OpenSheet,
    index: usize,
    rect: Rect,
    palette: &SheetPalette,
) {
    let target = Target::Button(index);
    let primary = index == open.dialog.default_button;
    let hovered = open.hovered == Some(target);
    let pressed = hovered && open.pressed == Some(target);
    let (fill, label) = match (primary, pressed, hovered) {
        (true, true, _) => (palette.primary.opacity(0.8), palette.primary_text),
        (true, false, true) => (palette.primary.opacity(0.9), palette.primary_text),
        (true, false, false) => (palette.primary, palette.primary_text),
        (false, true, _) => (palette.pressed, palette.menu.text),
        (false, false, true) => (palette.hovered, palette.menu.text),
        (false, false, false) => (palette.menu.highlight, palette.menu.text),
    };
    if open.focus_visible && open.focus == target {
        rounded(list, outset(rect, 3.0), CONTROL_RADIUS + 3.0, palette.ring);
        rounded(
            list,
            outset(rect, 1.5),
            CONTROL_RADIUS + 1.5,
            palette.menu.background,
        );
    }
    rounded(list, rect, CONTROL_RADIUS, fill);
    list.push(TextCommand {
        text: open.dialog.buttons[index].as_str().into(),
        x: rect.x,
        y: rect.y + (BUTTON_HEIGHT - MESSAGE_LINE) * 0.5,
        width: rect.width,
        height: MESSAGE_LINE,
        size: MESSAGE_SIZE,
        line_height: MESSAGE_LINE,
        color: label,
        align: TextAlign::Center,
        weight: if primary {
            TextWeight::Semibold
        } else {
            TextWeight::Regular
        },
    });
}

fn text(
    content: &str,
    rect: Rect,
    size: f32,
    line_height: f32,
    color: Color,
    weight: TextWeight,
) -> TextCommand {
    TextCommand {
        text: Arc::from(content),
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        size,
        line_height,
        color,
        align: TextAlign::Left,
        weight,
    }
}

fn rounded(list: &mut DisplayList, rect: Rect, radius: f32, color: Color) {
    list.push(RoundedRectCommand {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        radius,
        color,
    });
}

fn outset(rect: Rect, by: f32) -> Rect {
    Rect {
        x: rect.x - by,
        y: rect.y - by,
        width: rect.width + by * 2.0,
        height: rect.height + by * 2.0,
    }
}

/// The context menu's colors, and the few a dialog adds: the dimmed page,
/// the field, and the default button in the desktop's accent color.
struct SheetPalette {
    menu: MenuPalette,
    scrim: Color,
    secondary: Color,
    field: Color,
    primary: Color,
    primary_text: Color,
    hovered: Color,
    pressed: Color,
    ring: Color,
    selection: Color,
}

impl SheetPalette {
    fn new(menu: MenuPalette, accent: Option<[u8; 3]>, dark: bool) -> Self {
        let accent = accent.map(|[r, g, b]| Color::rgb8(r, g, b));
        let (primary, primary_text) = match accent {
            Some(accent) => (accent, Color::rgb8(255, 255, 255)),
            None => (menu.text, menu.background),
        };
        let ring = accent.unwrap_or(menu.text).opacity(0.55);
        Self {
            scrim: Color::rgba8(0, 0, 0, if dark { 92 } else { 46 }),
            secondary: menu.text.opacity(if dark { 0.72 } else { 0.68 }),
            field: if dark {
                Color::rgb8(29, 29, 32)
            } else {
                Color::rgb8(255, 255, 255)
            },
            selection: accent.unwrap_or(menu.text).opacity(0.28),
            hovered: menu.text.opacity(if dark { 0.15 } else { 0.12 }),
            pressed: menu.text.opacity(if dark { 0.22 } else { 0.18 }),
            primary,
            primary_text,
            ring,
            menu,
        }
    }
}
