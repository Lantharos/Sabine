use std::sync::Arc;

use crate::osr::protocol::{ContextMenuItem, MenuMark};
use crate::render::GpuRenderer;

pub(super) const MENU_PADDING: f32 = 5.0;
pub(super) const ITEM_HEIGHT: f32 = 26.0;
pub(super) const SEPARATOR_HEIGHT: f32 = 9.0;
pub(super) const TEXT_SIZE: f32 = 13.0;
pub(super) const TEXT_INSET: f32 = 12.0;
/// The room before labels for the marks of entries that turn on and off.
pub(super) const MARK_WIDTH: f32 = 16.0;
const SHORTCUT_GAP: f32 = 32.0;
const MIN_WIDTH: f32 = 180.0;
const EDGE_MARGIN: f32 = 4.0;
pub(super) const SUBMENU_ARROW: &str = "›";

/// One open level of the menu: the page's menu, or a submenu of it.
pub(super) struct MenuPanel {
    pub(super) entries: Vec<MenuEntry>,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) hovered: Option<usize>,
    pub(super) marks: bool,
}

pub(super) struct MenuEntry {
    pub(super) item: ContextMenuItem,
    pub(super) label: Arc<str>,
    pub(super) shortcut: Option<Arc<str>>,
    pub(super) top: f32,
    pub(super) height: f32,
}

impl MenuPanel {
    /// Lays the entries out, at the origin until the panel is placed.
    pub(super) fn new(items: Vec<ContextMenuItem>, renderer: &mut GpuRenderer) -> Self {
        let mut top = MENU_PADDING;
        let mut label_width = 0.0_f32;
        let mut trailing_width = 0.0_f32;
        let marks = items.iter().any(|item| item.mark != MenuMark::None);
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
                let trailing = shortcut
                    .as_deref()
                    .or((!item.items.is_empty()).then_some(SUBMENU_ARROW));
                if let Some(trailing) = trailing {
                    trailing_width = trailing_width.max(renderer.measure_text(trailing, TEXT_SIZE));
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
        let gap = if trailing_width > 0.0 {
            SHORTCUT_GAP
        } else {
            0.0
        };
        let mark_width = if marks { MARK_WIDTH } else { 0.0 };
        let width = (mark_width
            + label_width
            + gap
            + trailing_width
            + TEXT_INSET * 2.0
            + MENU_PADDING * 2.0)
            .ceil()
            .max(MIN_WIDTH);
        Self {
            entries,
            x: 0.0,
            y: 0.0,
            width,
            height: top + MENU_PADDING,
            hovered: None,
            marks,
        }
    }

    /// Opens the panel at `x`, `y`, or toward the other side of each edge
    /// it would cross, ending at `flip_x`, `flip_y`. A submenu of a panel
    /// that opened leftward keeps going left while it has room, so panels
    /// don't cover each other. Returns whether the panel opened leftward.
    pub(super) fn place(
        &mut self,
        (x, y): (f32, f32),
        (flip_x, flip_y): (f32, f32),
        (window_width, window_height): (f32, f32),
        leftward: bool,
    ) -> bool {
        let fits_right = x + self.width + EDGE_MARGIN <= window_width;
        let fits_left = flip_x - self.width >= EDGE_MARGIN;
        let opens_left = !fits_right || (leftward && fits_left);
        self.x = if opens_left {
            (flip_x - self.width).max(EDGE_MARGIN)
        } else {
            x
        };
        self.y = if y + self.height + EDGE_MARGIN <= window_height {
            y
        } else {
            (flip_y - self.height).max(EDGE_MARGIN)
        };
        opens_left
    }

    /// Where a submenu of the entry at `index` opens: beside it, over the
    /// panel's padding.
    pub(super) fn submenu_anchor(&self, index: usize) -> ((f32, f32), (f32, f32)) {
        let entry = &self.entries[index];
        let top = self.y + entry.top - MENU_PADDING;
        (
            (self.x + self.width - MENU_PADDING, top),
            (
                self.x + MENU_PADDING,
                top + entry.height + MENU_PADDING * 2.0,
            ),
        )
    }

    pub(super) fn entry_at(&self, x: f32, y: f32) -> Option<usize> {
        if !self.contains(x, y) {
            return None;
        }
        self.entries
            .iter()
            .position(|entry| y >= self.y + entry.top && y < self.y + entry.top + entry.height)
            .filter(|index| self.entries[*index].selectable())
    }

    pub(super) fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    pub(super) fn step(&self, forward: bool) -> Option<usize> {
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

    pub(super) fn selectable(&self) -> bool {
        self.entries.iter().any(MenuEntry::selectable)
    }
}

impl MenuEntry {
    pub(super) fn selectable(&self) -> bool {
        !self.item.separator && self.item.enabled
    }

    pub(super) fn opens_submenu(&self) -> bool {
        !self.item.items.is_empty()
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
