mod draw;
mod items;
mod panel;
mod popup;

use winit::keyboard::{Key, NamedKey};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::ContextMenuItem;
use crate::{MenuItem, MenuKind};
pub(in crate::osr::host) use items::Command;
use items::PanelItem;
use panel::MenuPanel;

const ARMING_DISTANCE: f32 = 3.0;
const CANCELLED: i32 = -1;

/// A menu the window draws over the page: the page's context menu, so it
/// matches the desktop instead of Chromium's own menus, which offscreen
/// pages lack, or a menu a page showed. Each open submenu adds a panel
/// beside the entry that opened it.
pub(in crate::osr::host) struct ContextMenu {
    panels: Vec<OpenPanel>,
    opened_at: (f32, f32),
    armed: bool,
    origin: MenuOrigin,
    /// The app's items the menu shows, which say what each of them does.
    app_items: Vec<MenuItem>,
}

/// Who waits to hear what was chosen.
enum MenuOrigin {
    /// Chromium, for the page's context menu.
    Chromium,
    /// A page's `menu.popup` call.
    Popup {
        browser_id: String,
        request_id: String,
    },
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

    fn command_at(&self, level: usize, index: usize) -> Option<Command> {
        let entry = &self.panels[level].panel.entries[index];
        (!entry.opens_submenu()).then(|| entry.item.command.clone())
    }
}

impl OsrNativeHost {
    /// Shows Chromium's context menu, followed by the app's own items on
    /// the app's page.
    pub(in crate::osr::host) fn open_context_menu(
        &mut self,
        x: i32,
        y: i32,
        guest: bool,
        items: Vec<ContextMenuItem>,
    ) {
        let app_items = if guest {
            Vec::new()
        } else {
            self.config.context_menu.clone()
        };
        let mut entries = items
            .into_iter()
            .map(PanelItem::from_chromium)
            .collect::<Vec<_>>();
        let own = items::app_items(&app_items, &self.config.title);
        if !entries.is_empty() && !own.is_empty() {
            entries.push(PanelItem::separator());
        }
        entries.extend(own);
        let anchor = (x as f32, y as f32 + self.titlebar_height());
        if !self.show_menu(anchor, entries, MenuOrigin::Chromium, app_items) {
            self.send_control(format!("context_menu\t{CANCELLED}\n"));
        }
    }

    /// Opens a menu at `anchor` in the window. Returns false when it has
    /// nothing to choose, or the window can't draw it yet.
    fn show_menu(
        &mut self,
        anchor: (f32, f32),
        entries: Vec<PanelItem>,
        origin: MenuOrigin,
        app_items: Vec<MenuItem>,
    ) -> bool {
        self.close_context_menu(None);
        if self.sheets.is_open() {
            return false;
        }
        let bounds = self.menu_bounds();
        let Some(renderer) = self.renderer.as_mut() else {
            return false;
        };
        let mut panel = MenuPanel::new(entries, renderer);
        if !panel.selectable() {
            return false;
        }
        let leftward = panel.place(anchor, anchor, bounds, false);
        self.context_menu = Some(ContextMenu {
            panels: vec![OpenPanel {
                panel,
                opened_from: None,
                leftward,
            }],
            opened_at: (self.cursor_x, self.cursor_y),
            armed: false,
            origin,
            app_items,
        });
        self.set_native_cursor(winit::cursor::CursorIcon::Default);
        self.request_menu_redraw();
        true
    }

    /// Closes the menu, running `choice` unless it is `None`.
    pub(in crate::osr::host) fn close_context_menu(&mut self, choice: Option<Command>) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        let app_choice = match &choice {
            Some(Command::App(id)) => Some(id.as_ref()),
            _ => None,
        };
        let kind = match menu.origin {
            MenuOrigin::Chromium => {
                let command = match choice {
                    Some(Command::Chromium(command)) => command,
                    _ => CANCELLED,
                };
                self.send_control(format!("context_menu\t{command}\n"));
                MenuKind::Context
            }
            MenuOrigin::Popup {
                browser_id,
                request_id,
            } => {
                self.send_bridge_response(
                    &browser_id,
                    &request_id,
                    Ok(serde_json::json!(app_choice)),
                );
                MenuKind::Popup
            }
        };
        if let Some(id) = app_choice {
            self.choose_menu_item(&menu.app_items, id, kind);
        }
        self.clear_native_cursor();
        self.request_menu_redraw();
    }

    pub(in crate::osr::host) fn dismiss_context_menu(&mut self) {
        if self
            .context_menu
            .take_if(|menu| matches!(menu.origin, MenuOrigin::Chromium))
            .is_some()
        {
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
            self.close_context_menu(None);
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
                Some(command) => self.close_context_menu(Some(command)),
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
                    Some(command) if activates => self.close_context_menu(Some(command)),
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
                self.close_context_menu(None);
                return true;
            }
            _ => return true,
        };
        self.hover_menu_entry(level, hovered, false);
        true
    }

    pub(in crate::osr::host) fn cancel_context_menu(&mut self) {
        self.close_context_menu(None);
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
