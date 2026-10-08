use std::ops::Range;

use crate::render::rect_pipeline::{
    ImageVertex, RectVertex, push_rect_command, push_rounded_rect_command,
};
use crate::render::{DisplayCommand, DisplayList};

use super::GpuRenderer;
use super::images::ImageDraw;

/// Geometry gathered for each frame, kept between frames so drawing does not
/// allocate. Draws keep the display list's order, so later commands such as
/// menus and tooltips cover the page.
#[derive(Default)]
pub(super) struct FrameGeometry {
    pub(super) rect_vertices: Vec<RectVertex>,
    pub(super) image_vertices: Vec<ImageVertex>,
    pub(super) draws: Vec<Draw>,
}

pub(super) enum Draw {
    Rects { vertices: Range<u32>, cutout: bool },
    Image(ImageDraw),
}

impl FrameGeometry {
    pub(super) fn has_cutouts(&self) -> bool {
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
}

impl GpuRenderer {
    pub(super) fn collect_geometry(&self, display_list: &DisplayList, frame: &mut FrameGeometry) {
        frame.rect_vertices.clear();
        frame.image_vertices.clear();
        frame.draws.clear();
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
                DisplayCommand::Text(_) => {}
            }
        }
    }
}
