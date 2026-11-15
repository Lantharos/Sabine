use super::WindowRegionRect;

pub(super) fn rounded_rect(width: i32, height: i32, radius: i32) -> Vec<WindowRegionRect> {
    let radius = radius.min(width / 2).min(height / 2).max(0);
    let mut shape = Shape::default();
    for span in rounded_spans(0, height, height, radius) {
        shape.add(span.inset, span, width - span.inset * 2);
    }
    shape.0
}

pub(super) fn rounded_left(width: i32, height: i32, radius: i32) -> Vec<WindowRegionRect> {
    let radius = radius.min(width).min(height / 2).max(0);
    let mut shape = Shape::default();
    for span in rounded_spans(0, height, height, radius) {
        shape.add(span.inset, span, width - span.inset);
    }
    shape.0
}

pub(super) fn titlebar_and_sidebar(
    width: i32,
    height: i32,
    sidebar_width: i32,
    titlebar_height: i32,
    radius: i32,
) -> Vec<WindowRegionRect> {
    let width = width.max(0);
    let height = height.max(0);
    let sidebar_width = sidebar_width.clamp(0, width);
    let titlebar_height = titlebar_height.clamp(0, height);
    let radius = radius.min(width / 2).min(height / 2).max(0);
    let mut shape = Shape::default();
    for span in rounded_spans(0, titlebar_height, height, radius) {
        shape.add(span.inset, span, width - span.inset * 2);
    }
    for span in rounded_spans(titlebar_height, height, height, radius) {
        shape.add(span.inset, span, sidebar_width - span.inset);
    }
    shape.0
}

pub(super) fn content_after_sidebar(
    width: i32,
    height: i32,
    sidebar_width: i32,
    titlebar_height: i32,
) -> Vec<WindowRegionRect> {
    let width = width.max(0);
    let height = height.max(0);
    let sidebar_width = sidebar_width.clamp(0, width);
    let titlebar_height = titlebar_height.clamp(0, height);
    vec![WindowRegionRect::new(
        sidebar_width,
        titlebar_height,
        width - sidebar_width,
        height - titlebar_height,
    )]
}

pub(super) fn content_after_sidebar_rounded_right(
    width: i32,
    height: i32,
    sidebar_width: i32,
    titlebar_height: i32,
    radius: i32,
) -> Vec<WindowRegionRect> {
    let width = width.max(0);
    let height = height.max(0);
    let sidebar_width = sidebar_width.clamp(0, width);
    let titlebar_height = titlebar_height.clamp(0, height);
    let radius = radius
        .min((width - sidebar_width) / 2)
        .min(height / 2)
        .max(0);
    let mut shape = Shape::default();
    for span in rounded_spans(titlebar_height, height, height, radius) {
        shape.add(sidebar_width, span, width - sidebar_width - span.inset);
    }
    shape.0
}

/// Rows of a shape that share how far its rounded corners inset them.
#[derive(Clone, Copy)]
struct Span {
    y: i32,
    height: i32,
    inset: i32,
}

/// The rows from `top` to `bottom` of a shape `height` tall whose corners are
/// rounded with `radius`: one span per row along the corners, and one for the
/// straight rows between them.
fn rounded_spans(top: i32, bottom: i32, height: i32, radius: i32) -> impl Iterator<Item = Span> {
    let top_corner = (top..radius.min(bottom)).map(move |y| Span {
        y,
        height: 1,
        inset: corner_inset(radius - y, radius),
    });
    let straight = radius.max(top)..(height - radius).min(bottom);
    let straight = (!straight.is_empty()).then(|| Span {
        y: straight.start,
        height: straight.end - straight.start,
        inset: 0,
    });
    let bottom_corner = (height - radius).max(top)..bottom;
    let bottom_corner = bottom_corner.map(move |y| Span {
        y,
        height: 1,
        inset: corner_inset(y - (height - radius - 1), radius),
    });
    top_corner.chain(straight).chain(bottom_corner)
}

/// How far a corner of `radius` insets the row `distance` rows from the
/// corner's center.
fn corner_inset(distance: i32, radius: i32) -> i32 {
    let distance = f64::from(distance);
    let radius = f64::from(radius);
    (radius - (radius * radius - distance * distance).max(0.0).sqrt()).ceil() as i32
}

/// Rects built row span by row span, with spans that line up joined into one.
#[derive(Default)]
struct Shape(Vec<WindowRegionRect>);

impl Shape {
    fn add(&mut self, x: i32, span: Span, width: i32) {
        let rect = WindowRegionRect::new(x, span.y, width, span.height);
        if rect.is_empty() {
            return;
        }
        match self.0.last_mut() {
            Some(last)
                if last.x == rect.x
                    && last.width == rect.width
                    && last.y + last.height == rect.y =>
            {
                last.height += rect.height;
            }
            _ => self.0.push(rect),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::WindowRegion;

    fn covers(rects: &[WindowRegionRect], x: i32, y: i32) -> bool {
        rects.iter().any(|rect| {
            (rect.x..rect.x + rect.width).contains(&x)
                && (rect.y..rect.y + rect.height).contains(&y)
        })
    }

    #[test]
    fn rounded_rects_keep_straight_rows_in_one_rect() {
        let region = rounded_rect(400, 300, 12);
        assert!(region.len() <= 12 * 2 + 1);
        assert!(region.iter().any(|rect| rect.height == 300 - 24));
        assert!(!covers(&region, 0, 0));
        assert!(covers(&region, 12, 0));
        assert!(covers(&region, 0, 150));
        assert!(!covers(&region, 399, 299));
    }

    #[test]
    fn zoom_moves_page_edges_but_keeps_window_corners() {
        let zoom = 1.5;
        let sidebar = WindowRegion::rounded_left(260, 15).zoomed(zoom);
        let blur = sidebar.resolved_rects(1200, 800);
        assert!(covers(&blur, 389, 400));
        assert!(!covers(&blur, 390, 400));
        assert!(!covers(&blur, 0, 0));
        assert!(covers(&blur, 15, 0));

        let content = WindowRegion::content_after_sidebar_rounded_right(260, 0, 15).zoomed(zoom);
        let opaque = content.resolved_rects(1200, 800);
        assert!(!covers(&opaque, 389, 400));
        assert!(covers(&opaque, 390, 400));
        assert!(covers(&opaque, 1184, 0));
        assert!(!covers(&opaque, 1199, 0));
    }

    #[test]
    fn titlebar_and_sidebar_round_only_the_outer_corners() {
        let region = titlebar_and_sidebar(800, 600, 260, 38, 14);
        assert!(!covers(&region, 0, 0));
        assert!(!covers(&region, 799, 0));
        assert!(covers(&region, 799, 37));
        assert!(!covers(&region, 400, 300));
        assert!(covers(&region, 259, 599));
        assert!(!covers(&region, 0, 599));
    }
}
