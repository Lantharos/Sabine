use std::sync::Arc;

use super::panel::{
    MARK_WIDTH, MENU_PADDING, MenuEntry, MenuPanel, SEPARATOR_HEIGHT, SUBMENU_ARROW, TEXT_INSET,
    TEXT_SIZE,
};
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::ui::surface::{Palette, RADIUS};
use crate::render::{
    DisplayList, RectCommand, RoundedRectCommand, TextAlign, TextCommand, TextWeight,
};
use crate::window::style::Color;

const LINE_HEIGHT: f32 = 18.0;

impl OsrNativeHost {
    pub(in crate::osr::host) fn draw_popup_menu(&self, list: &mut DisplayList) {
        let Some(menu) = &self.popup_menu else {
            return;
        };
        let palette = Palette::new(self.appearance());
        for open in &menu.panels {
            draw_panel(list, &open.panel, &palette);
        }
    }
}

fn draw_panel(list: &mut DisplayList, panel: &MenuPanel, palette: &Palette) {
    list.push(RoundedRectCommand {
        x: panel.x - 1.0,
        y: panel.y - 1.0,
        width: panel.width + 2.0,
        height: panel.height + 2.0,
        radius: RADIUS + 1.0,
        color: palette.border,
    });
    list.push(RoundedRectCommand {
        x: panel.x,
        y: panel.y,
        width: panel.width,
        height: panel.height,
        radius: RADIUS,
        color: palette.background,
    });
    for (index, entry) in panel.entries.iter().enumerate() {
        let top = panel.y + entry.top;
        if entry.item.separator {
            list.push(RectCommand {
                x: panel.x + MENU_PADDING + TEXT_INSET * 0.5,
                y: top + SEPARATOR_HEIGHT * 0.5,
                width: panel.width - (MENU_PADDING + TEXT_INSET * 0.5) * 2.0,
                height: 1.0,
                color: palette.separator,
            });
            continue;
        }
        if panel.hovered == Some(index) {
            list.push(RoundedRectCommand {
                x: panel.x + MENU_PADDING,
                y: top,
                width: panel.width - MENU_PADDING * 2.0,
                height: entry.height,
                radius: RADIUS - MENU_PADDING + 1.0,
                color: palette.highlight,
            });
        }
        draw_entry(list, panel, entry, palette);
    }
}

fn draw_entry(list: &mut DisplayList, panel: &MenuPanel, entry: &MenuEntry, palette: &Palette) {
    let color = if entry.item.enabled {
        palette.text
    } else {
        palette.disabled
    };
    let inset_x = panel.x + MENU_PADDING + TEXT_INSET;
    let mark_width = if panel.marks { MARK_WIDTH } else { 0.0 };
    let text_width = panel.width - (MENU_PADDING + TEXT_INSET) * 2.0;
    let text = |text: Arc<str>, x: f32, width: f32, color: Color, align: TextAlign| TextCommand {
        text,
        x,
        y: panel.y + entry.top + (entry.height - LINE_HEIGHT) * 0.5,
        width,
        height: LINE_HEIGHT,
        size: TEXT_SIZE,
        line_height: LINE_HEIGHT,
        color,
        align,
        weight: TextWeight::Regular,
    };
    if let Some(mark) = mark_glyph(entry) {
        list.push(text(
            mark.into(),
            inset_x,
            mark_width,
            color,
            TextAlign::Left,
        ));
    }
    list.push(text(
        Arc::clone(&entry.label),
        inset_x + mark_width,
        text_width - mark_width,
        color,
        TextAlign::Left,
    ));
    let trailing = entry
        .shortcut
        .clone()
        .or_else(|| entry.opens_submenu().then(|| SUBMENU_ARROW.into()));
    if let Some(trailing) = trailing {
        list.push(text(
            trailing,
            inset_x,
            text_width,
            palette.disabled,
            TextAlign::Right,
        ));
    }
}

fn mark_glyph(entry: &MenuEntry) -> Option<&'static str> {
    (entry.item.checkable && entry.item.checked).then_some("✓")
}
