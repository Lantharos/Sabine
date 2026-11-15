use std::ops::Range;

use super::Target;
use super::field::TextField;
use crate::MessageDialog;
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::ui::surface::{BUTTON_HEIGHT, LINE_HEIGHT, Rect, TEXT_SIZE};
use crate::render::{GpuRenderer, TextWeight};

pub(super) const PADDING: f32 = 20.0;
pub(super) const TITLE_SIZE: f32 = 15.0;
pub(super) const TITLE_LINE: f32 = 20.0;
pub(super) const FIELD_HEIGHT: f32 = 30.0;
pub(super) const FIELD_INSET: f32 = 9.0;
const MAX_WIDTH: f32 = 380.0;
const EDGE_MARGIN: f32 = 16.0;
const MESSAGE_GAP: f32 = 6.0;
const FIELD_GAP: f32 = 14.0;
const BUTTONS_GAP: f32 = 20.0;
const BUTTON_INSET: f32 = 14.0;
const BUTTON_MIN_WIDTH: f32 = 72.0;
const BUTTON_SPACING: f32 = 8.0;

/// Where a sheet's parts sit for one size of the window, and the part of
/// its field's text that fits the field.
pub(super) struct SheetLayout {
    /// The window's width, height and titlebar the sheet was laid out in.
    bounds: (f32, f32, f32),
    pub(super) panel: Rect,
    pub(super) title: Option<Rect>,
    pub(super) message: Option<Rect>,
    pub(super) field: Option<Rect>,
    pub(super) buttons: Vec<Rect>,
    /// Where each character of the field's text, as shown, starts.
    pub(super) offsets: Vec<(usize, f32)>,
    /// The bytes of the field's shown text that fit in the field.
    pub(super) view: Range<usize>,
}

impl SheetLayout {
    fn new(
        dialog: &MessageDialog,
        field: Option<&TextField>,
        renderer: &mut GpuRenderer,
        bounds: (f32, f32, f32),
    ) -> Self {
        let (width, height, top) = bounds;
        let labels = dialog
            .buttons
            .iter()
            .map(|label| {
                (renderer.measure_text(label, TEXT_SIZE) + BUTTON_INSET * 2.0)
                    .ceil()
                    .max(BUTTON_MIN_WIDTH)
            })
            .collect::<Vec<_>>();
        let row = labels.iter().sum::<f32>() + BUTTON_SPACING * (labels.len() - 1) as f32;
        let panel_width = MAX_WIDTH
            .max(row + PADDING * 2.0)
            .min(width - EDGE_MARGIN * 2.0)
            .max(1.0);
        let text_width = (panel_width - PADDING * 2.0).max(1.0);
        let mut cursor = PADDING;
        let mut block = |text: &str, size: f32, line: f32, weight: TextWeight, gap: f32| {
            if text.is_empty() {
                return None;
            }
            let height = renderer.measure_text_height(text, size, line, text_width, weight);
            if cursor > PADDING {
                cursor += gap;
            }
            let rect = Rect {
                x: PADDING,
                y: cursor,
                width: text_width,
                height,
            };
            cursor += height;
            Some(rect)
        };
        let title = block(
            &dialog.title,
            TITLE_SIZE,
            TITLE_LINE,
            TextWeight::Semibold,
            0.0,
        );
        let mut message = block(
            &dialog.message,
            TEXT_SIZE,
            LINE_HEIGHT,
            TextWeight::Regular,
            MESSAGE_GAP,
        );
        let controls =
            field.map_or(0.0, |_| FIELD_GAP + FIELD_HEIGHT) + BUTTONS_GAP + BUTTON_HEIGHT + PADDING;
        let room = (height - top - EDGE_MARGIN * 2.0).max(1.0);
        if let Some(message) = message.as_mut() {
            let overflow = cursor + controls - room;
            if overflow > 0.0 {
                message.height = (message.height - overflow).max(LINE_HEIGHT);
                cursor = message.y + message.height;
            }
        }
        let panel_height = cursor + controls;
        let origin_x = ((width - panel_width) * 0.5).max(0.0);
        let origin_y = top + ((height - top - panel_height) * 0.38).max(EDGE_MARGIN);
        let place = |rect: Rect| Rect {
            x: rect.x + origin_x,
            y: rect.y + origin_y,
            ..rect
        };
        let field_rect = field.map(|_| {
            place(Rect {
                x: PADDING,
                y: cursor + FIELD_GAP,
                width: text_width,
                height: FIELD_HEIGHT,
            })
        });
        let buttons_y = origin_y + panel_height - PADDING - BUTTON_HEIGHT;
        let mut button_x = origin_x + panel_width - PADDING - row;
        let buttons = labels
            .into_iter()
            .map(|width| {
                let rect = Rect {
                    x: button_x,
                    y: buttons_y,
                    width,
                    height: BUTTON_HEIGHT,
                };
                button_x += width + BUTTON_SPACING;
                rect
            })
            .collect();
        let mut layout = Self {
            bounds,
            panel: Rect {
                x: origin_x,
                y: origin_y,
                width: panel_width,
                height: panel_height,
            },
            title: title.map(place),
            message: message.map(place),
            field: field_rect,
            buttons,
            offsets: Vec::new(),
            view: 0..0,
        };
        if let Some(field) = field {
            layout.fit_field(field, renderer);
        }
        layout
    }

    /// Measures the field's text again and scrolls it so the caret shows.
    pub(super) fn fit_field(&mut self, field: &TextField, renderer: &mut GpuRenderer) {
        let Some(rect) = self.field else {
            return;
        };
        let shown = field.shown();
        self.offsets = renderer.measure_text_offsets(&shown, TEXT_SIZE);
        let inner = rect.width - FIELD_INSET * 2.0;
        let caret = field.caret + field.preedit.len();
        let mut start = self.view.start.min(caret);
        while self.x_at(caret) - self.x_at(start) > inner {
            start = self.next_offset(start);
        }
        while let Some(previous) = self.previous_offset(start)
            && self.x_at(shown.len()) - self.x_at(previous) <= inner
        {
            start = previous;
        }
        let left = self.x_at(start);
        let end = self
            .offsets
            .iter()
            .filter(|(byte, x)| *byte >= start && x - left <= inner)
            .map(|(byte, _)| *byte)
            .next_back()
            .unwrap_or(start);
        self.view = start..end;
    }

    /// Where the character at `byte` of the field's shown text starts.
    pub(super) fn x_at(&self, byte: usize) -> f32 {
        self.offsets
            .iter()
            .rev()
            .find(|(start, _)| *start <= byte)
            .map_or(0.0, |(_, x)| *x)
    }

    /// The window position of `byte` of the field's shown text.
    pub(super) fn field_x(&self, byte: usize) -> f32 {
        self.field.map_or(0.0, |rect| {
            rect.x + FIELD_INSET + self.x_at(byte.clamp(self.view.start, self.view.end))
                - self.x_at(self.view.start)
        })
    }

    /// The character boundary of the field's text nearest a window position.
    pub(super) fn field_offset_at(&self, x: f32) -> usize {
        let target = x - self.field_x(self.view.start);
        let left = self.x_at(self.view.start);
        self.offsets
            .iter()
            .filter(|(byte, _)| self.view.contains(byte) || *byte == self.view.end)
            .min_by(|(_, a), (_, b)| {
                ((a - left) - target)
                    .abs()
                    .total_cmp(&((b - left) - target).abs())
            })
            .map_or(self.view.start, |(byte, _)| *byte)
    }

    pub(super) fn target_at(&self, x: f32, y: f32) -> Option<Target> {
        if self.field.is_some_and(|field| field.contains(x, y)) {
            return Some(Target::Field);
        }
        self.buttons
            .iter()
            .position(|button| button.contains(x, y))
            .map(Target::Button)
    }

    fn next_offset(&self, byte: usize) -> usize {
        self.offsets
            .iter()
            .map(|(start, _)| *start)
            .find(|start| *start > byte)
            .unwrap_or(byte)
    }

    fn previous_offset(&self, byte: usize) -> Option<usize> {
        self.offsets
            .iter()
            .rev()
            .map(|(start, _)| *start)
            .find(|start| *start < byte)
    }
}

impl OsrNativeHost {
    /// Lays the open sheet out for the window's size, unless it already is.
    pub(in crate::osr::host) fn lay_out_sheet(&mut self) {
        let bounds = (
            self.logical_width(),
            self.logical_height(),
            self.titlebar_height(),
        );
        let (Some(open), Some(renderer)) = (self.sheets.open.as_mut(), self.renderer.as_mut())
        else {
            return;
        };
        if open
            .layout
            .as_ref()
            .is_some_and(|layout| layout.bounds == bounds)
        {
            return;
        }
        let view = open.layout.take().map(|layout| layout.view);
        let mut layout = SheetLayout::new(&open.dialog, open.field.as_ref(), renderer, bounds);
        if let (Some(view), Some(field)) = (view, open.field.as_ref()) {
            layout.view = view;
            layout.fit_field(field, renderer);
        }
        open.layout = Some(layout);
        self.place_sheet_ime();
    }

    /// Measures the field after an edit and keeps its caret in sight.
    pub(super) fn refit_sheet_field(&mut self) {
        let (Some(open), Some(renderer)) = (self.sheets.open.as_mut(), self.renderer.as_mut())
        else {
            return;
        };
        if let (Some(layout), Some(field)) = (open.layout.as_mut(), open.field.as_ref()) {
            layout.fit_field(field, renderer);
        }
        self.place_sheet_ime();
        self.request_sheet_redraw();
    }

    /// Lends the input method to the field, where its caret is.
    fn place_sheet_ime(&mut self) {
        let Some(open) = self.sheets.open.as_mut() else {
            return;
        };
        let (Some(layout), Some(field)) = (open.layout.as_ref(), open.field.as_ref()) else {
            return;
        };
        let Some(rect) = layout.field else {
            return;
        };
        let area = (
            layout.field_x(field.caret),
            rect.y + 4.0,
            1.0,
            rect.height - 8.0,
        );
        let lent = std::mem::replace(&mut open.ime_lent, true);
        self.lend_ime(area, lent);
    }
}
