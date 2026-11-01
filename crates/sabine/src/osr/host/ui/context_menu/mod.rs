mod draw;
mod panel;

use winit::keyboard::{Key, NamedKey};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::ContextMenuItem;
use panel::MenuPanel;

const ARMING_DISTANCE: f32 = 3.0;
const CANCELLED: i32 = -1;

/// The page's context menu, drawn by the window over the page so it matches
/// the desktop instead of Chromium's own menus, which offscreen pages lack.
/// Each open submenu adds a panel beside the entry that opened it.
pub(in crate::osr::host) struct ContextMenu {
    panels: Vec<OpenPanel>,
    opened_at: (f32, f32),
    armed: bool,
}

struct OpenPanel {
    panel: MenuPanel,
    /// The entry of the panel before this one that opened it.
    opened_from: Option<usize>,
    leftward: bool,
}

impl ContextMenu {
    fn panel_at(&self, x: f32, y: f32) -> Option<usize> {
        self.panels
            .iter()
            .rposition(|open| open.panel.contains(x, y))
    }

    fn deepest(&mut self) -> &mut MenuPanel {
        &mut self
            .panels
            .last_mut()
            .expect("an open menu has a panel")
            .panel
    }

    fn command_at(&self, level: usize, index: usize) -> Option<i32> {
        let entry = &self.panels[level].panel.entries[index];
        (!entry.opens_submenu()).then_some(entry.item.id)
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
        let mut panel = MenuPanel::new(items, renderer);
        if !panel.selectable() {
            self.send_control(format!("context_menu\t{CANCELLED}\n"));
            return;
        }
        let anchor = (x as f32, y as f32 + self.titlebar_height());
        let leftward = panel.place(anchor, anchor, self.menu_bounds(), false);
        self.context_menu = Some(ContextMenu {
            panels: vec![OpenPanel {
                panel,
                opened_from: None,
                leftward,
            }],
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
        match menu.panel_at(x, y) {
            Some(level) => {
                let hovered = menu.panels[level].panel.entry_at(x, y);
                self.hover_menu_entry(level, hovered, true);
            }
            None => {
                let deepest = menu.deepest();
                if deepest.hovered.take().is_some() {
                    self.request_menu_redraw();
                }
            }
        }
        true
    }

    pub(in crate::osr::host) fn menu_pointer_pressed(&mut self, x: f32, y: f32) -> bool {
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        if menu.panel_at(x, y).is_some() {
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
        let chosen = menu
            .panel_at(x, y)
            .filter(|_| menu.armed)
            .and_then(|level| Some((level, menu.panels[level].panel.entry_at(x, y)?)));
        if let Some((level, index)) = chosen {
            match menu.command_at(level, index) {
                Some(command) => self.close_context_menu(command),
                None => self.hover_menu_entry(level, Some(index), true),
            }
        }
        true
    }

    pub(in crate::osr::host) fn menu_key_pressed(&mut self, key: &Key) -> bool {
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        let level = menu.panels.len() - 1;
        let activates = matches!(key, Key::Named(NamedKey::Enter))
            || matches!(key, Key::Character(text) if text == " ");
        if activates || matches!(key, Key::Named(NamedKey::ArrowRight)) {
            if let Some(index) = menu.deepest().hovered {
                match menu.command_at(level, index) {
                    Some(command) if activates => self.close_context_menu(command),
                    Some(_) => {}
                    None => self.enter_submenu(level, index),
                }
            }
            return true;
        }
        let deepest = menu.deepest();
        let hovered = match key {
            Key::Named(NamedKey::ArrowDown) => deepest.step(true),
            Key::Named(NamedKey::ArrowUp) => deepest.step(false),
            Key::Named(NamedKey::Home) => {
                deepest.hovered = None;
                deepest.step(true)
            }
            Key::Named(NamedKey::End) => {
                deepest.hovered = None;
                deepest.step(false)
            }
            Key::Named(NamedKey::ArrowLeft | NamedKey::Escape) if level > 0 => {
                menu.panels.pop();
                self.request_menu_redraw();
                return true;
            }
            Key::Named(NamedKey::Escape | NamedKey::Tab) => {
                self.close_context_menu(CANCELLED);
                return true;
            }
            _ => return true,
        };
        self.hover_menu_entry(level, hovered, false);
        true
    }

    pub(in crate::osr::host) fn cancel_context_menu(&mut self) {
        self.close_context_menu(CANCELLED);
    }

    /// Hovers an entry of the panel at `level`, closing submenus of other
    /// entries, and opening the entry's own when `open` asks to, as the
    /// pointer does while the keyboard waits to be asked.
    fn hover_menu_entry(&mut self, level: usize, hovered: Option<usize>, open: bool) {
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        let open_child = menu
            .panels
            .get(level + 1)
            .and_then(|child| child.opened_from);
        let keeps_child = open_child.is_some() && open_child == hovered;
        let opens = hovered
            .filter(|index| open && !keeps_child && menu.command_at(level, *index).is_none());
        let panel = &mut menu.panels[level].panel;
        if panel.hovered == hovered && opens.is_none() && (keeps_child || open_child.is_none()) {
            return;
        }
        panel.hovered = hovered;
        if !keeps_child {
            menu.panels.truncate(level + 1);
        }
        if let Some(index) = opens {
            self.open_submenu(level, index);
        }
        self.request_menu_redraw();
    }

    /// Opens the submenu of an entry from the keyboard, with its first entry
    /// hovered.
    fn enter_submenu(&mut self, level: usize, index: usize) {
        self.hover_menu_entry(level, Some(index), true);
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        if menu.panels.len() > level + 1 {
            let child = menu.deepest();
            child.hovered = None;
            child.hovered = child.step(true);
            self.request_menu_redraw();
        }
    }

    fn open_submenu(&mut self, level: usize, index: usize) {
        let bounds = self.menu_bounds();
        let (Some(menu), Some(renderer)) = (self.context_menu.as_mut(), self.renderer.as_mut())
        else {
            return;
        };
        let parent = &menu.panels[level];
        let items = parent.panel.entries[index].item.items.clone();
        let (anchor, flip) = parent.panel.submenu_anchor(index);
        let mut panel = MenuPanel::new(items, renderer);
        let leftward = panel.place(anchor, flip, bounds, parent.leftward);
        menu.panels.push(OpenPanel {
            panel,
            opened_from: Some(index),
            leftward,
        });
    }

    fn menu_bounds(&self) -> (f32, f32) {
        (self.logical_width(), self.logical_height())
    }

    fn request_menu_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
