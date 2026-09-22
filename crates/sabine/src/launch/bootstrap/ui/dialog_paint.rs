use super::{BG, FILL, MUTED, TEXT, confirm::Dialog, fill_rect};
use crate::render::raster_text::blend_rect;
use glyphon::cosmic_text::{Align, Scroll};
use glyphon::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap};

type Rect = (i32, i32, i32, i32);

pub(super) struct Document {
    fonts: FontSystem,
    cache: SwashCache,
    body: Buffer,
    label: Buffer,
    layout: Option<(i32, f32)>,
    content_height: f32,
}

impl Document {
    pub(super) fn new(text: &str) -> Self {
        let mut fonts = FontSystem::new();
        let mut body = Buffer::new(&mut fonts, Metrics::new(14.0, 21.0));
        body.set_wrap(Wrap::WordOrGlyph);
        body.set_text(
            text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            None,
        );
        let label = Buffer::new(&mut fonts, Metrics::new(14.0, 21.0));
        Self {
            fonts,
            cache: SwashCache::new(),
            body,
            label,
            layout: None,
            content_height: 0.0,
        }
    }

    fn body(
        &mut self,
        pixels: &mut [u32],
        surface: (u32, u32),
        bounds: Rect,
        scale: f32,
        scroll: &mut f32,
    ) -> f32 {
        if self.layout != Some((bounds.2, scale)) {
            self.body.set_metrics_and_size(
                Metrics::new(14.0 * scale, 22.0 * scale),
                Some(bounds.2 as f32),
                None,
            );
            self.body.set_scroll(Scroll::default());
            self.body.shape_until_scroll(&mut self.fonts, false);
            self.content_height = self
                .body
                .layout_runs()
                .map(|run| run.line_top + run.line_height)
                .last()
                .unwrap_or(0.0);
            self.layout = Some((bounds.2, scale));
        }
        let max_scroll = (self.content_height - bounds.3 as f32).max(0.0);
        *scroll = scroll.clamp(0.0, max_scroll);
        self.body
            .set_size(Some(bounds.2 as f32), Some(bounds.3 as f32));
        self.body.set_scroll(Scroll::new(0, *scroll, 0.0));
        self.body.draw(
            &mut self.fonts,
            &mut self.cache,
            color(TEXT),
            |x, y, w, h, c| {
                glyph(pixels, surface, bounds, (x, y, w, h), c);
            },
        );
        max_scroll
    }

    #[allow(clippy::too_many_arguments)]
    fn label(
        &mut self,
        pixels: &mut [u32],
        surface: (u32, u32),
        bounds: Rect,
        text: &str,
        size: f32,
        tint: u32,
        center: bool,
    ) {
        self.label.set_metrics_and_size(
            Metrics::new(size, size + 6.0),
            Some(bounds.2 as f32),
            Some(bounds.3 as f32),
        );
        self.label.set_wrap(Wrap::WordOrGlyph);
        self.label.set_scroll(Scroll::default());
        self.label.set_text(
            text,
            &Attrs::new().family(Family::SansSerif),
            Shaping::Advanced,
            Some(if center { Align::Center } else { Align::Left }),
        );
        self.label.draw(
            &mut self.fonts,
            &mut self.cache,
            color(tint),
            |x, y, w, h, c| {
                glyph(pixels, surface, bounds, (x, y, w, h), c);
            },
        );
    }
}

fn color(value: u32) -> Color {
    Color::rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

fn glyph(
    pixels: &mut [u32],
    surface: (u32, u32),
    bounds: Rect,
    glyph: (i32, i32, u32, u32),
    color: Color,
) {
    let (x, y, w, h) = glyph;
    let left = x.max(0);
    let top = y.max(0);
    let right = (x + w as i32).min(bounds.2);
    let bottom = (y + h as i32).min(bounds.3);
    if right > left && bottom > top {
        blend_rect(
            bytemuck::cast_slice_mut(pixels),
            surface,
            (
                bounds.0 + left,
                bounds.1 + top,
                (right - left) as u32,
                (bottom - top) as u32,
            ),
            color.as_rgba(),
        );
    }
}

pub(super) fn paint(app: &mut Dialog, pixels: &mut [u32], surface: (u32, u32)) {
    pixels.fill(BG);
    let scale = app.scale();
    let unit = |value: f32| (value * scale).round() as i32;
    let width = surface.0 as i32;
    let height = surface.1 as i32;
    let pad = unit(28.0);
    app.document.label(
        pixels,
        surface,
        (pad, unit(24.0), width - pad * 2, unit(66.0)),
        &app.title,
        24.0 * scale,
        TEXT,
        false,
    );
    if app.notice {
        app.document.label(
            pixels,
            surface,
            (pad, unit(100.0), width - pad * 2, unit(24.0)),
            "What happened",
            14.0 * scale,
            MUTED,
            false,
        );
    }
    let panel_top = if app.notice { 134.0 } else { 100.0 };
    let panel = (
        pad,
        unit(panel_top),
        width - pad * 2,
        height - unit(panel_top + 120.0),
    );
    if app.notice {
        rounded_rect(pixels, surface, panel, unit(10.0), 0xFF_20_20_23);
    }
    let inset = if app.notice { unit(16.0) } else { 0 };
    let bounds = (
        panel.0 + inset,
        panel.1 + inset,
        panel.2 - inset * 2 - unit(8.0),
        panel.3 - inset * 2,
    );
    app.max_scroll = app
        .document
        .body(pixels, surface, bounds, scale, &mut app.scroll);
    if app.max_scroll > 0.0 {
        let track = panel.3 - inset * 2;
        let thumb = ((track as f32 * bounds.3 as f32 / (bounds.3 as f32 + app.max_scroll)) as i32)
            .max(unit(20.0));
        let y = (app.scroll / app.max_scroll * (track - thumb) as f32) as i32;
        rounded_rect(
            pixels,
            surface,
            (
                panel.0 + panel.2 - unit(9.0),
                panel.1 + inset + y,
                unit(3.0),
                thumb,
            ),
            unit(1.0),
            MUTED,
        );
    }
    let hint = if !app.status.is_empty() {
        app.status.as_str()
    } else if app.max_scroll > 0.0 {
        "Scroll or use ↑ ↓ to read more."
    } else if app.notice {
        "Open logs for the full diagnostic files."
    } else {
        ""
    };
    app.document.label(
        pixels,
        surface,
        (pad, height - unit(109.0), width - pad * 2, unit(28.0)),
        hint,
        12.0 * scale,
        MUTED,
        false,
    );
    for index in 0..2 {
        let rect = app.button_rect(index);
        let focused = app.focus == index;
        if focused {
            let ring = unit(2.0);
            rounded_rect(
                pixels,
                surface,
                (
                    rect.0 - ring * 2,
                    rect.1 - ring * 2,
                    rect.2 + ring * 4,
                    rect.3 + ring * 4,
                ),
                unit(10.0),
                MUTED,
            );
            rounded_rect(
                pixels,
                surface,
                (
                    rect.0 - ring,
                    rect.1 - ring,
                    rect.2 + ring * 2,
                    rect.3 + ring * 2,
                ),
                unit(8.0),
                BG,
            );
        }
        let hovered = app.hovered() == Some(index);
        let pressed = hovered && app.pressed == Some(index);
        let tint = match (index == 1, pressed, hovered) {
            (true, true, _) => 0xFF_BB_BB_BF,
            (true, false, true) => 0xFF_FF_FF_FF,
            (true, _, _) => FILL,
            (false, true, _) => 0xFF_46_46_4B,
            (false, false, true) => 0xFF_3A_3A_40,
            _ => 0xFF_2A_2A_2E,
        };
        rounded_rect(pixels, surface, rect, unit(6.0), tint);
        let label = match (app.notice, index) {
            (true, 0) => "Open logs",
            (true, _) => "Close",
            (false, 0) => "Later",
            _ => "Install update",
        };
        app.document.label(
            pixels,
            surface,
            (rect.0, rect.1 + unit(10.0), rect.2, rect.3 - unit(10.0)),
            label,
            14.0 * scale,
            if index == 1 { BG } else { TEXT },
            true,
        );
    }
}

fn rounded_rect(pixels: &mut [u32], surface: (u32, u32), rect: Rect, radius: i32, tint: u32) {
    let (x, y, w, h) = rect;
    let radius = radius.min(w / 2).min(h / 2).max(0);
    for row in 0..h {
        let dy = if row < radius {
            radius - row - 1
        } else if row >= h - radius {
            row - (h - radius)
        } else {
            0
        };
        let inset = if dy == 0 {
            0
        } else {
            radius - ((radius * radius - dy * dy) as f32).sqrt() as i32
        };
        fill_rect(
            pixels,
            surface,
            (x + inset, y + row, w - inset * 2, 1),
            tint,
        );
    }
}
