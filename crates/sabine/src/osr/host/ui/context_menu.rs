use std::sync::Arc;

use winit::{
    keyboard::{Key, NamedKey},
    window::Theme,
};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::ContextMenuItem;
use crate::render::{DisplayList, RectCommand, RoundedRectCommand, TextAlign, TextCommand};
use crate::window::style::Color;

const MENU_PADDING: f32 = 5.0;
const ITEM_HEIGHT: f32 = 26.0;
const SEPARATOR_HEIGHT: f32 = 9.0;
const TEXT_SIZE: f32 = 13.0;
const TEXT_INSET: f32 = 12.0;
const SHORTCUT_GAP: f32 = 32.0;
const MIN_WIDTH: f32 = 180.0;
const RADIUS: f32 = 8.0;
const EDGE_MARGIN: f32 = 4.0;
const ARMING_DISTANCE: f32 = 3.0;
const CANCELLED: i32 = -1;

/// The page's context menu, drawn by the window over the page so it matches
/// the desktop instead of Chromium's own menus, which offscreen pages lack.
pub(in crate::osr::host) struct ContextMenu {
    entries: Vec<MenuEntry>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    hovered: Option<usize>,
    opened_at: (f32, f32),
    armed: bool,
}

struct MenuEntry {
    item: ContextMenuItem,
    label: Arc<str>,
    shortcut: Option<Arc<str>>,
    top: f32,
    height: f32,
}

impl ContextMenu {
    fn entry_at(&self, x: f32, y: f32) -> Option<usize> {
        if x < self.x || x >= self.x + self.width {
            return None;
        }
        self.entries
            .iter()
            .position(|entry| y >= self.y + entry.top && y < self.y + entry.top + entry.height)
            .filter(|index| self.entries[*index].selectable())
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    fn step(&self, forward: bool) -> Option<usize> {
        let count = self.entries.len();
        let start = self.hovered.unwrap_or(if forward { count - 1 } else { 0 });
        (1..=count)
            .map(|offset| {
                if forward {
                    (start + offset) % count
                } else {
                    (start + count - offset) % count
                }
            })
            .find(|index| self.entries[*index].selectable())
    }
}

impl MenuEntry {
    fn selectable(&self) -> bool {
        !self.item.separator && self.item.enabled
    }
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn open_context_menu(
        &mut self,
        x: i32,
        y: i32,
        items: Vec<ContextMenuItem>,
    ) {
        self.close_context_menu(CANCELLED);
        let Some(renderer) = self.renderer.as_mut() else {
            self.send_control(format!("context_menu\t{CANCELLED}\n"));
            return;
        };
        let mut top = MENU_PADDING;
        let mut label_width = 0.0_f32;
        let mut shortcut_width = 0.0_f32;
        let entries = items
            .into_iter()
            .map(|item| {
                let height = if item.separator {
                    SEPARATOR_HEIGHT
                } else {
                    ITEM_HEIGHT
                };
                let label: Arc<str> = item.label.as_str().into();
                let shortcut = shortcut_for(item.id).map(Arc::<str>::from);
                if !item.separator {
                    label_width = label_width.max(renderer.measure_text(&label, TEXT_SIZE));
                }
                if let Some(shortcut) = &shortcut {
                    shortcut_width = shortcut_width.max(renderer.measure_text(shortcut, TEXT_SIZE));
                }
                let entry = MenuEntry {
                    item,
                    label,
                    shortcut,
                    top,
                    height,
                };
                top += height;
                entry
            })
            .collect::<Vec<_>>();
        if entries.iter().all(|entry| !entry.selectable()) {
            self.send_control(format!("context_menu\t{CANCELLED}\n"));
            return;
        }
        let gap = if shortcut_width > 0.0 {
            SHORTCUT_GAP
        } else {
            0.0
        };
        let width = (label_width + gap + shortcut_width + TEXT_INSET * 2.0 + MENU_PADDING * 2.0)
            .ceil()
            .max(MIN_WIDTH);
        let height = top + MENU_PADDING;
        let window_width = self.logical_width();
        let window_height = self.logical_height();
        let anchor_x = x as f32;
        let anchor_y = y as f32 + self.titlebar_height();
        let menu_x = if anchor_x + width + EDGE_MARGIN <= window_width {
            anchor_x
        } else {
            (anchor_x - width).max(EDGE_MARGIN)
        };
        let menu_y = if anchor_y + height + EDGE_MARGIN <= window_height {
            anchor_y
        } else {
            (anchor_y - height).max(EDGE_MARGIN)
        };
        self.context_menu = Some(ContextMenu {
            entries,
            x: menu_x,
            y: menu_y,
            width,
            height,
            hovered: None,
            opened_at: (self.cursor_x, self.cursor_y),
            armed: false,
        });
        self.set_native_cursor(winit::cursor::CursorIcon::Default);
        self.request_menu_redraw();
    }

    /// Closes the menu, running `command` unless it is the cancellation id.
    pub(in crate::osr::host) fn close_context_menu(&mut self, command: i32) {
        if self.context_menu.take().is_none() {
            return;
        }
        self.send_control(format!("context_menu\t{command}\n"));
        self.clear_native_cursor();
        self.request_menu_redraw();
    }

    pub(in crate::osr::host) fn dismiss_context_menu(&mut self) {
        if self.context_menu.take().is_some() {
            self.clear_native_cursor();
            self.request_menu_redraw();
        }
    }

    /// Returns false when no menu is open, so the pointer reaches the page.
    pub(in crate::osr::host) fn menu_pointer_moved(&mut self, x: f32, y: f32) -> bool {
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        self.cursor_x = x;
        self.cursor_y = y;
        let (opened_x, opened_y) = menu.opened_at;
        menu.armed |=
            (x - opened_x).abs() > ARMING_DISTANCE || (y - opened_y).abs() > ARMING_DISTANCE;
        let hovered = menu.entry_at(x, y);
        if hovered != menu.hovered {
            menu.hovered = hovered;
            self.request_menu_redraw();
        }
        true
    }

    pub(in crate::osr::host) fn menu_pointer_pressed(&mut self, x: f32, y: f32) -> bool {
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        if menu.contains(x, y) {
            menu.armed = true;
        } else {
            self.close_context_menu(CANCELLED);
        }
        true
    }

    pub(in crate::osr::host) fn menu_pointer_released(&mut self, x: f32, y: f32) -> bool {
        let Some(menu) = self.context_menu.as_ref() else {
            return false;
        };
        if menu.armed
            && let Some(index) = menu.entry_at(x, y)
        {
            let command = menu.entries[index].item.id;
            self.close_context_menu(command);
        }
        true
    }

    pub(in crate::osr::host) fn menu_key_pressed(&mut self, key: &Key) -> bool {
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        let activates = matches!(key, Key::Named(NamedKey::Enter))
            || matches!(key, Key::Character(text) if text == " ");
        if activates {
            if let Some(index) = menu.hovered {
                let command = menu.entries[index].item.id;
                self.close_context_menu(command);
            }
            return true;
        }
        let hovered = match key {
            Key::Named(NamedKey::ArrowDown) => menu.step(true),
            Key::Named(NamedKey::ArrowUp) => menu.step(false),
            Key::Named(NamedKey::Home) => {
                menu.hovered = None;
                menu.step(true)
            }
            Key::Named(NamedKey::End) => {
                menu.hovered = None;
                menu.step(false)
            }
            Key::Named(NamedKey::Escape | NamedKey::Tab) => {
                self.close_context_menu(CANCELLED);
                return true;
            }
            _ => return true,
        };
        if hovered != menu.hovered {
            menu.hovered = hovered;
            self.request_menu_redraw();
        }
        true
    }

    pub(in crate::osr::host) fn cancel_context_menu(&mut self) {
        self.close_context_menu(CANCELLED);
    }

    pub(in crate::osr::host) fn draw_context_menu(&self, list: &mut DisplayList) {
        let Some(menu) = &self.context_menu else {
            return;
        };
        let palette =
            MenuPalette::for_theme(self.window.as_ref().and_then(|window| window.theme()));
        list.push(RoundedRectCommand {
            x: menu.x - 1.0,
            y: menu.y - 1.0,
            width: menu.width + 2.0,
            height: menu.height + 2.0,
            radius: RADIUS + 1.0,
            color: palette.border,
        });
        list.push(RoundedRectCommand {
            x: menu.x,
            y: menu.y,
            width: menu.width,
            height: menu.height,
            radius: RADIUS,
            color: palette.background,
        });
        for (index, entry) in menu.entries.iter().enumerate() {
            let top = menu.y + entry.top;
            if entry.item.separator {
                list.push(RectCommand {
                    x: menu.x + MENU_PADDING + TEXT_INSET * 0.5,
                    y: top + SEPARATOR_HEIGHT * 0.5,
                    width: menu.width - (MENU_PADDING + TEXT_INSET * 0.5) * 2.0,
                    height: 1.0,
                    color: palette.separator,
                });
                continue;
            }
            if menu.hovered == Some(index) {
                list.push(RoundedRectCommand {
                    x: menu.x + MENU_PADDING,
                    y: top,
                    width: menu.width - MENU_PADDING * 2.0,
                    height: entry.height,
                    radius: RADIUS - MENU_PADDING + 1.0,
                    color: palette.highlight,
                });
            }
            let color = if entry.item.enabled {
                palette.text
            } else {
                palette.disabled
            };
            let text_x = menu.x + MENU_PADDING + TEXT_INSET;
            let text_width = menu.width - (MENU_PADDING + TEXT_INSET) * 2.0;
            let text_y = top + (entry.height - 18.0) * 0.5;
            list.push(TextCommand {
                text: Arc::clone(&entry.label),
                x: text_x,
                y: text_y,
                width: text_width,
                height: 18.0,
                size: TEXT_SIZE,
                line_height: 18.0,
                color,
                align: TextAlign::Left,
            });
            if let Some(shortcut) = &entry.shortcut {
                list.push(TextCommand {
                    text: Arc::clone(shortcut),
                    x: text_x,
                    y: text_y,
                    width: text_width,
                    height: 18.0,
                    size: TEXT_SIZE,
                    line_height: 18.0,
                    color: palette.disabled,
                    align: TextAlign::Right,
                });
            }
        }
    }

    fn request_menu_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
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
    fn for_theme(theme: Option<Theme>) -> Self {
        match theme {
            Some(Theme::Light) => Self {
                background: Color::rgb8(250, 250, 251),
                border: Color::rgba8(0, 0, 0, 38),
                separator: Color::rgba8(0, 0, 0, 26),
                highlight: Color::rgba8(0, 0, 0, 18),
                text: Color::rgb8(24, 24, 27),
                disabled: Color::rgba8(24, 24, 27, 110),
            },
            _ => Self {
                background: Color::rgb8(37, 37, 41),
                border: Color::rgba8(255, 255, 255, 30),
                separator: Color::rgba8(255, 255, 255, 24),
                highlight: Color::rgba8(255, 255, 255, 22),
                text: Color::rgb8(240, 240, 242),
                disabled: Color::rgba8(240, 240, 242, 105),
            },
        }
    }
}

/// The keyboard shortcut shown beside a CEF edit command, in the desktop's
/// own notation.
fn shortcut_for(command: i32) -> Option<&'static str> {
    let shortcuts: [&str; 6] = if cfg!(target_os = "macos") {
        ["⌘Z", "⇧⌘Z", "⌘X", "⌘C", "⌘V", "⌘A"]
    } else if cfg!(windows) {
        ["Ctrl+Z", "Ctrl+Y", "Ctrl+X", "Ctrl+C", "Ctrl+V", "Ctrl+A"]
    } else {
        [
            "Ctrl+Z",
            "Ctrl+Shift+Z",
            "Ctrl+X",
            "Ctrl+C",
            "Ctrl+V",
            "Ctrl+A",
        ]
    };
    match command {
        110 => Some(shortcuts[0]),
        111 => Some(shortcuts[1]),
        112 => Some(shortcuts[2]),
        113 => Some(shortcuts[3]),
        114 => Some(shortcuts[4]),
        117 => Some(shortcuts[5]),
        _ => None,
    }
}
