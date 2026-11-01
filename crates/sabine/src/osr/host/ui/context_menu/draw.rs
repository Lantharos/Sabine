use std::sync::Arc;

use super::panel::{
    MARK_WIDTH, MENU_PADDING, MenuEntry, MenuPanel, SEPARATOR_HEIGHT, SUBMENU_ARROW, TEXT_INSET,
    TEXT_SIZE,
};
use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::MenuMark;
use crate::render::{DisplayList, RectCommand, RoundedRectCommand, TextAlign, TextCommand};
use crate::window::style::Color;

const RADIUS: f32 = 8.0;
const LINE_HEIGHT: f32 = 18.0;

impl OsrNativeHost {
    pub(in crate::osr::host) fn draw_context_menu(&self, list: &mut DisplayList) {
        let Some(menu) = &self.context_menu else {
            return;
        };
        let palette = MenuPalette::new(self.appearance().is_none_or(|appearance| appearance.dark));
        for open in &menu.panels {
            draw_panel(list, &open.panel, &palette);
        }
    }
}

fn draw_panel(list: &mut DisplayList, panel: &MenuPanel, palette: &MenuPalette) {
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

fn draw_entry(list: &mut DisplayList, panel: &MenuPanel, entry: &MenuEntry, palette: &MenuPalette) {
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
    if !entry.item.checked {
        return None;
    }
    match entry.item.mark {
        MenuMark::Check => Some("✓"),
        MenuMark::Radio => Some("•"),
        MenuMark::None => None,
    }
}

struct MenuPalette {
    background: Color,
    border: Color,
    separator: Color,
    highlight: Color,
    text: Color,
    disabled: Color,
}

impl MenuPalette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                background: Color::rgb8(37, 37, 41),
                border: Color::rgba8(255, 255, 255, 30),
                separator: Color::rgba8(255, 255, 255, 24),
                highlight: Color::rgba8(255, 255, 255, 22),
                text: Color::rgb8(240, 240, 242),
                disabled: Color::rgba8(240, 240, 242, 105),
            }
        } else {
            Self {
                background: Color::rgb8(250, 250, 251),
                border: Color::rgba8(0, 0, 0, 38),
                separator: Color::rgba8(0, 0, 0, 26),
                highlight: Color::rgba8(0, 0, 0, 18),
                text: Color::rgb8(24, 24, 27),
                disabled: Color::rgba8(24, 24, 27, 110),
            }
        }
    }
}
