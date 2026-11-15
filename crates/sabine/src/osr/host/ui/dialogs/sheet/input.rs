use winit::cursor::CursorIcon;
use winit::event::{Ime, KeyEvent};
use winit::keyboard::{Key, NamedKey};

use super::Target;
use crate::MenuRole;
use crate::clipboard::{ClipboardContent, Selection};
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::types::OsrHostEvent;

const PLAIN_TEXT: &str = "text/plain";

impl OsrNativeHost {
    /// Takes a key pressed while a sheet is open, which the page never sees.
    pub(in crate::osr::host) fn sheet_key_pressed(&mut self, event: &KeyEvent) {
        let shift = self.modifiers.shift_key();
        let Some(open) = self.sheets.open.as_mut() else {
            return;
        };
        let focused_button = match open.focus {
            Target::Button(index) => Some(index),
            Target::Field => None,
        };
        let space = matches!(&event.logical_key, Key::Character(text) if text == " ");
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                let cancel = open.dialog.cancel_button;
                self.answer_sheet(cancel);
            }
            Key::Named(NamedKey::Enter) => {
                let button = focused_button.unwrap_or(open.dialog.default_button);
                self.answer_sheet(button);
            }
            Key::Named(NamedKey::Tab) => self.move_sheet_focus(!shift),
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) if focused_button.is_some() => {
                let forward = event.logical_key == Key::Named(NamedKey::ArrowRight);
                let buttons = open.dialog.buttons.len();
                let index = focused_button.unwrap_or_default();
                open.focus_visible = true;
                open.focus = Target::Button(if forward {
                    (index + 1).min(buttons - 1)
                } else {
                    index.saturating_sub(1)
                });
                self.request_sheet_redraw();
            }
            _ if space && focused_button.is_some() => {
                self.answer_sheet(focused_button.unwrap_or_default());
            }
            _ if focused_button.is_none() => self.edit_sheet_field(event),
            _ => {}
        }
    }

    fn move_sheet_focus(&mut self, forward: bool) {
        let Some(open) = self.sheets.open.as_mut() else {
            return;
        };
        let targets = open
            .field
            .as_ref()
            .map(|_| Target::Field)
            .into_iter()
            .chain((0..open.dialog.buttons.len()).map(Target::Button))
            .collect::<Vec<_>>();
        let current = targets
            .iter()
            .position(|target| *target == open.focus)
            .unwrap_or_default();
        let count = targets.len();
        open.focus_visible = true;
        open.focus = targets[if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        }];
        self.request_sheet_redraw();
    }

    fn edit_sheet_field(&mut self, event: &KeyEvent) {
        let primary = if cfg!(target_os = "macos") {
            self.modifiers.meta_key()
        } else {
            self.modifiers.control_key()
        };
        let word = if cfg!(target_os = "macos") {
            self.modifiers.alt_key()
        } else {
            self.modifiers.control_key()
        };
        let types = !self.modifiers.alt_key() || cfg!(target_os = "macos");
        let shift = self.modifiers.shift_key();
        let Some(field) = self
            .sheets
            .open
            .as_mut()
            .and_then(|open| open.field.as_mut())
        else {
            return;
        };
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => field.delete_backward(word),
            Key::Named(NamedKey::Delete) => field.delete_forward(word),
            Key::Named(NamedKey::ArrowLeft) => field.step(false, word, shift),
            Key::Named(NamedKey::ArrowRight) => field.step(true, word, shift),
            Key::Named(NamedKey::Home | NamedKey::ArrowUp) => field.move_to(0, shift),
            Key::Named(NamedKey::End | NamedKey::ArrowDown) => {
                field.move_to(field.text.len(), shift);
            }
            Key::Character(key) if primary => match key.to_lowercase().as_str() {
                "a" => field.select_all(),
                "c" => {
                    let copied = field.selected_text().to_string();
                    self.copy_sheet_text(copied);
                    return;
                }
                "x" => {
                    let cut = field.selected_text().to_string();
                    field.insert("");
                    self.copy_sheet_text(cut);
                }
                "v" => {
                    self.paste_into_sheet();
                    return;
                }
                _ => return,
            },
            _ => match event.text.as_deref() {
                Some(text) if types && !text.chars().any(char::is_control) => field.insert(text),
                _ => return,
            },
        }
        self.refit_sheet_field();
    }

    /// Runs an edit item of the app's menus on the open sheet, which holds
    /// the keyboard in place of the page. Returns false while none is open.
    pub(in crate::osr::host) fn edit_sheet(&mut self, role: MenuRole) -> bool {
        let Some(open) = self.sheets.open.as_mut() else {
            return false;
        };
        let Some(field) = open.field.as_mut() else {
            return true;
        };
        match role {
            MenuRole::SelectAll => field.select_all(),
            MenuRole::Copy => {
                let copied = field.selected_text().to_string();
                self.copy_sheet_text(copied);
                return true;
            }
            MenuRole::Cut => {
                let cut = field.selected_text().to_string();
                field.insert("");
                self.copy_sheet_text(cut);
            }
            MenuRole::Paste => {
                self.paste_into_sheet();
                return true;
            }
            _ => return true,
        }
        self.refit_sheet_field();
        true
    }

    fn copy_sheet_text(&self, text: String) {
        let Some(clipboard) = &self.clipboard else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let mut content = ClipboardContent::default();
        content.push(PLAIN_TEXT, text.into_bytes());
        if let Err(error) = clipboard.write(Selection::Clipboard, content) {
            sabine_runtime::report_error("clipboard", error);
        }
    }

    fn paste_into_sheet(&self) {
        let Some(clipboard) = &self.clipboard else {
            return;
        };
        let sender = self.sender.clone();
        let proxy = self.proxy.clone();
        clipboard.read(
            Selection::Clipboard,
            Some(vec![PLAIN_TEXT.to_string()]),
            Box::new(move |content| {
                let text = content.ok().and_then(|content| {
                    content
                        .items()
                        .find_map(|(_, bytes)| std::str::from_utf8(bytes).ok().map(str::to_string))
                });
                if let Some(text) = text
                    && sender.send(OsrHostEvent::SheetPaste(text)).is_ok()
                {
                    proxy.wake_up();
                }
            }),
        );
    }

    pub(in crate::osr::host) fn paste_sheet_text(&mut self, text: &str) {
        if let Some(field) = self
            .sheets
            .open
            .as_mut()
            .and_then(|open| open.field.as_mut())
        {
            field.insert(text);
            self.refit_sheet_field();
        }
    }

    pub(in crate::osr::host) fn sheet_ime(&mut self, ime: Ime) {
        let Some(field) = self
            .sheets
            .open
            .as_mut()
            .and_then(|open| open.field.as_mut())
        else {
            return;
        };
        match ime {
            Ime::Commit(text) => {
                field.preedit.clear();
                field.insert(&text);
            }
            Ime::Preedit(text, _) => field.preedit = text,
            Ime::Disabled => field.preedit.clear(),
            Ime::Enabled | Ime::DeleteSurrounding { .. } => return,
        }
        self.refit_sheet_field();
    }

    /// Returns false when no sheet takes the pointer, as over the titlebar,
    /// so it reaches the window's controls.
    pub(in crate::osr::host) fn sheet_pointer_moved(&mut self, x: f32, y: f32) -> bool {
        let titlebar = self.titlebar_height();
        let Some(open) = self.sheets.open.as_mut() else {
            return false;
        };
        if y < titlebar && !open.selecting {
            return false;
        }
        let Some(layout) = &open.layout else {
            return true;
        };
        let hovered = layout.target_at(x, y);
        let selected = open
            .field
            .as_mut()
            .filter(|_| open.selecting)
            .map(|field| field.move_to(layout.field_offset_at(x), true))
            .is_some();
        let cursor = if hovered == Some(Target::Field) {
            CursorIcon::Text
        } else {
            CursorIcon::Default
        };
        let changed = std::mem::replace(&mut open.hovered, hovered) != hovered;
        self.set_native_cursor(cursor);
        if selected {
            self.refit_sheet_field();
        } else if changed {
            self.request_sheet_redraw();
        }
        true
    }

    pub(in crate::osr::host) fn sheet_pointer_button(
        &mut self,
        pressed: bool,
        x: f32,
        y: f32,
    ) -> bool {
        let titlebar = self.titlebar_height();
        let shift = self.modifiers.shift_key();
        let Some(open) = self.sheets.open.as_mut() else {
            return false;
        };
        if y < titlebar && open.pressed.is_none() && !open.selecting {
            return false;
        }
        let Some(layout) = &open.layout else {
            return true;
        };
        let target = layout.target_at(x, y);
        if pressed {
            open.pressed = target;
            if target == Some(Target::Field)
                && let Some(field) = open.field.as_mut()
            {
                open.focus = Target::Field;
                open.selecting = true;
                field.move_to(layout.field_offset_at(x), shift);
                self.refit_sheet_field();
            } else {
                self.request_sheet_redraw();
            }
            return true;
        }
        open.selecting = false;
        match open.pressed.take() {
            Some(Target::Button(index)) if target == Some(Target::Button(index)) => {
                self.answer_sheet(index);
            }
            _ => self.request_sheet_redraw(),
        }
        true
    }
}
