use std::sync::Arc;

use super::PADDING;
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::ui::surface::{Button, LINE_HEIGHT, Palette, TEXT_SIZE, button, panel};
use crate::render::{DisplayList, TextAlign, TextCommand, TextWeight};

impl OsrNativeHost {
    pub(in crate::osr::host) fn draw_permission_prompt(&self, list: &mut DisplayList) {
        let (Some(prompt), Some(frame)) = (&self.permission_prompt, self.prompt_frame()) else {
            return;
        };
        let palette = Palette::new(self.appearance());
        panel(list, frame, &palette);
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
        for (index, shown) in prompt.buttons.iter().enumerate() {
            let shown = Button {
                rect: prompt.button_frame(frame, index),
                label: &shown.label,
                primary: shown.allows,
                hovered: prompt.hovered == Some(index),
                pressed: prompt.pressed == Some(index),
                focused: false,
            };
            button(list, &shown, &palette);
        }
    }
}
