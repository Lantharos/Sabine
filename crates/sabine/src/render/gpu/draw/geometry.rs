use std::ops::Range;

use crate::render::rect_pipeline::{
    ImageVertex, RectVertex, push_rect_command, push_rounded_rect_command,
};
use crate::render::{DisplayCommand, DisplayList};

use crate::render::gpu::GpuRenderer;
use crate::render::gpu::images::ImageDraw;

/// Geometry gathered for each frame, kept between frames so drawing does not
/// allocate. Draws keep the display list's order, so later commands such as
/// menus and tooltips cover the page.
#[derive(Default)]
pub(in crate::render::gpu) struct FrameGeometry {
    pub(in crate::render::gpu) rect_vertices: Vec<RectVertex>,
    pub(in crate::render::gpu) image_vertices: Vec<ImageVertex>,
    pub(in crate::render::gpu) draws: Vec<Draw>,
    /// How many of the frame's text commands come before the end of each
    /// text layer.
    pub(in crate::render::gpu) text_layer_ends: Vec<usize>,
}

pub(in crate::render::gpu) enum Draw {
    Rects { vertices: Range<u32>, cutout: bool },
    Image(ImageDraw),
    Text(usize),
}

impl FrameGeometry {
    pub(in crate::render::gpu) fn has_cutouts(&self) -> bool {
        self.draws
            .iter()
            .any(|draw| matches!(draw, Draw::Rects { cutout: true, .. }))
    }

    fn push_rects(&mut self, start: u32, cutout: bool) {
        let end = self.rect_vertices.len() as u32;
        match self.draws.last_mut() {
            Some(Draw::Rects {
                vertices,
                cutout: batched,
            }) if *batched == cutout => vertices.end = end,
            _ => self.draws.push(Draw::Rects {
                vertices: start..end,
                cutout,
            }),
        }
    }

    fn push_text(&mut self) {
        if let Some(&Draw::Text(layer)) = self.draws.last() {
            self.text_layer_ends[layer] += 1;
            return;
        }
        let end = self.text_layer_ends.last().copied().unwrap_or_default() + 1;
        self.draws.push(Draw::Text(self.text_layer_ends.len()));
        self.text_layer_ends.push(end);
    }
}

impl GpuRenderer {
    pub(in crate::render::gpu) fn collect_geometry(
        &self,
        display_list: &DisplayList,
        frame: &mut FrameGeometry,
    ) {
        frame.rect_vertices.clear();
        frame.image_vertices.clear();
        frame.draws.clear();
        frame.text_layer_ends.clear();
        for command in &display_list.commands {
            let start = frame.rect_vertices.len() as u32;
            match command {
                DisplayCommand::Rect(command) => {
                    push_rect_command(&mut frame.rect_vertices, command, self.scale_factor);
                    frame.push_rects(start, false);
                }
                DisplayCommand::RoundedRect(command) => {
                    push_rounded_rect_command(&mut frame.rect_vertices, command, self.scale_factor);
                    frame.push_rects(start, false);
                }
                DisplayCommand::Cutout(command) => {
                    push_rounded_rect_command(&mut frame.rect_vertices, command, self.scale_factor);
                    frame.push_rects(start, true);
                }
                DisplayCommand::Image(image) => {
                    if let Some(draw) = self.image_draw(image, &mut frame.image_vertices) {
                        frame.draws.push(Draw::Image(draw));
                    }
                }
                DisplayCommand::Text(_) => frame.push_text(),
            }
        }
    }
}
