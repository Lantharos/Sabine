use std::sync::Arc;

use super::{BUTTON_HEIGHT, LINE_HEIGHT, PADDING, TEXT_SIZE};
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::ui::context_menu::draw::{MenuPalette, RADIUS};
use crate::render::{DisplayList, RoundedRectCommand, TextAlign, TextCommand, TextWeight};
use crate::window::style::Color;

const BUTTON_RADIUS: f32 = 7.0;
const ACCENT: [u8; 3] = [53, 132, 228];

impl OsrNativeHost {
    pub(in crate::osr::host) fn draw_permission_prompt(&self, list: &mut DisplayList) {
        let (Some(prompt), Some(frame)) = (&self.permission_prompt, self.prompt_frame()) else {
            return;
        };
        let appearance = self.appearance();
        let palette = MenuPalette::new(appearance.is_none_or(|appearance| appearance.dark));
        let [r, g, b] = appearance
            .and_then(|appearance| appearance.accent)
            .unwrap_or(ACCENT);
        let accent = Color::rgb8(r, g, b);
        list.push(RoundedRectCommand {
            x: frame.x - 1.0,
            y: frame.y - 1.0,
            width: frame.width + 2.0,
            height: frame.height + 2.0,
            radius: RADIUS + 1.0,
            color: palette.border,
        });
        list.push(RoundedRectCommand {
            x: frame.x,
            y: frame.y,
            width: frame.width,
            height: frame.height,
            radius: RADIUS,
            color: palette.background,
        });
        list.push(TextCommand {
            text: Arc::clone(&prompt.message),
            x: frame.x + PADDING,
            y: frame.y + PADDING,
            width: frame.width - PADDING * 2.0,
            height: prompt.text_height,
            size: TEXT_SIZE,
            line_height: LINE_HEIGHT,
            color: palette.text,
            align: TextAlign::Left,
            weight: TextWeight::Regular,
        });
        for (index, button) in prompt.buttons.iter().enumerate() {
            let area = prompt.button_frame(frame, index);
            let active = prompt.hovered == Some(index) || prompt.pressed == Some(index);
            let (fill, text) = if button.allows {
                (
                    accent.opacity(if active { 0.85 } else { 1.0 }),
                    Color::WHITE,
                )
            } else {
                let highlight = palette.highlight;
                let alpha = if active {
                    highlight.a * 1.8
                } else {
                    highlight.a
                };
                (
                    Color::rgba(highlight.r, highlight.g, highlight.b, alpha),
                    palette.text,
                )
            };
            list.push(RoundedRectCommand {
                x: area.x,
                y: area.y,
                width: area.width,
                height: area.height,
                radius: BUTTON_RADIUS,
                color: fill,
            });
            list.push(TextCommand {
                text: Arc::clone(&button.label),
                x: area.x,
                y: area.y + (BUTTON_HEIGHT - LINE_HEIGHT) * 0.5,
                width: area.width,
                height: LINE_HEIGHT,
                size: TEXT_SIZE,
                line_height: LINE_HEIGHT,
                color: text,
                align: TextAlign::Center,
                weight: TextWeight::Regular,
            });
        }
    }
}
