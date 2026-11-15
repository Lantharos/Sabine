use super::layout::{FIELD_INSET, SheetLayout, TITLE_LINE, TITLE_SIZE};
use super::{OpenSheet, Target};
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::ui::surface::{
    Button, LINE_HEIGHT, Palette, Rect, TEXT_SIZE, button, panel, rounded,
};
use crate::render::{DisplayList, RectCommand, TextAlign, TextCommand, TextWeight};
use crate::window::style::Color;

const FIELD_RADIUS: f32 = 6.0;
const CARET_INSET: f32 = 7.0;

impl OsrNativeHost {
    pub(in crate::osr::host) fn draw_sheet(&self, list: &mut DisplayList, width: f32, height: f32) {
        let Some(open) = &self.sheets.open else {
            return;
        };
        let Some(layout) = &open.layout else {
            return;
        };
        let palette = Palette::new(self.appearance());
        let top = self.titlebar_height();
        list.push(RectCommand {
            x: 0.0,
            y: top,
            width,
            height: height - top,
            color: Color::rgba8(0, 0, 0, if palette.dark { 92 } else { 46 }),
        });
        panel(list, layout.panel, &palette);
        if let Some(title) = layout.title {
            list.push(text(
                &open.dialog.title,
                title,
                TITLE_SIZE,
                TITLE_LINE,
                palette.text,
                TextWeight::Semibold,
            ));
        }
        if let Some(message) = layout.message {
            list.push(text(
                &open.dialog.message,
                message,
                TEXT_SIZE,
                LINE_HEIGHT,
                palette.text.opacity(0.7),
                TextWeight::Regular,
            ));
        }
        draw_field(list, open, layout, &palette);
        for (index, rect) in layout.buttons.iter().copied().enumerate() {
            let target = Some(Target::Button(index));
            let button_shown = Button {
                rect,
                label: &open.dialog.buttons[index],
                primary: index == open.dialog.default_button,
                hovered: open.hovered == target,
                pressed: open.pressed == target,
                focused: open.focus_visible && Some(open.focus) == target,
            };
            button(list, &button_shown, &palette);
        }
    }
}

fn draw_field(list: &mut DisplayList, open: &OpenSheet, layout: &SheetLayout, palette: &Palette) {
    let (Some(field), Some(rect)) = (&open.field, layout.field) else {
        return;
    };
    let focused = open.focus == Target::Field;
    let border = if focused {
        palette.ring()
    } else {
        palette.border
    };
    let fill = if palette.dark {
        Color::rgb8(29, 29, 32)
    } else {
        Color::WHITE
    };
    rounded(list, rect.outset(1.0), FIELD_RADIUS + 1.0, border);
    rounded(list, rect, FIELD_RADIUS, fill);
    let line = Rect {
        x: rect.x + FIELD_INSET,
        y: rect.y + (rect.height - LINE_HEIGHT) * 0.5,
        width: rect.width - FIELD_INSET * 2.0,
        height: LINE_HEIGHT,
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
            TEXT_SIZE,
            LINE_HEIGHT,
            palette.disabled,
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
            color: palette.accent.unwrap_or(palette.text).opacity(0.28),
        });
    }
    list.push(text(
        &shown[layout.view.clone()],
        Rect {
            width: line.width + 2.0,
            ..line
        },
        TEXT_SIZE,
        LINE_HEIGHT,
        palette.text,
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
            color: palette.text,
        });
    }
    if focused && selection.is_empty() {
        list.push(RectCommand {
            x: layout.field_x(field.caret + field.preedit.len()).round(),
            y: rect.y + CARET_INSET,
            width: 1.0,
            height: rect.height - CARET_INSET * 2.0,
            color: palette.text,
        });
    }
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
        text: content.into(),
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
