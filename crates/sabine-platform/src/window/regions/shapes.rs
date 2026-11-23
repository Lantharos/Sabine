use super::{WindowInsets, WindowRegionRect};

pub(super) fn rounded_rect(width: i32, height: i32, radius: i32) -> Vec<WindowRegionRect> {
    let radius = radius.min(width / 2).min(height / 2).max(0);
    let mut shape = Shape::default();
    for span in rounded_spans(0, height, height, radius) {
        shape.add_span(span.inset, span, width - span.inset * 2);
    }
    shape.0
}

pub(super) fn rounded_left(width: i32, height: i32, radius: i32) -> Vec<WindowRegionRect> {
    let radius = radius.min(width).min(height / 2).max(0);
    let mut shape = Shape::default();
    for span in rounded_spans(0, height, height, radius) {
        shape.add_span(span.inset, span, width - span.inset);
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
        shape.add_span(span.inset, span, width - span.inset * 2);
    }
    for span in rounded_spans(titlebar_height, height, height, radius) {
        shape.add_span(span.inset, span, sidebar_width - span.inset);
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
        shape.add_span(sidebar_width, span, width - sidebar_width - span.inset);
    }
    shape.0
}

pub(super) fn inset(
    width: i32,
    height: i32,
    insets: WindowInsets,
    radius: i32,
) -> Vec<WindowRegionRect> {
    let mut shape = Shape::default();
    for row in sheet_rows(width, height, insets, radius) {
        shape.add(row.left, row.y, row.right - row.left, row.height);
    }
    shape.0
}

pub(super) fn around_inset(
    width: i32,
    height: i32,
    radius: i32,
    insets: WindowInsets,
    inset_radius: i32,
) -> Vec<WindowRegionRect> {
    let sheet = sheet_rows(width, height, insets, inset_radius).collect::<Vec<_>>();
    let mut shape = Shape::default();
    for row in box_rows(0, 0, width, height, radius) {
        let mut y = row.y;
        while y < row.bottom() {
            let next = sheet.iter().find(|sheet| sheet.bottom() > y);
            let end = match next {
                Some(sheet) if sheet.y <= y => sheet.bottom(),
                Some(sheet) => sheet.y,
                None => row.bottom(),
            }
            .min(row.bottom());
            match next.filter(|sheet| sheet.y <= y) {
                Some(sheet) => {
                    let left_end = sheet.left.clamp(row.left, row.right);
                    let right_start = sheet.right.clamp(left_end, row.right);
                    shape.add(row.left, y, left_end - row.left, end - y);
                    shape.add(right_start, y, row.right - right_start, end - y);
                }
                None => shape.add(row.left, y, row.right - row.left, end - y),
            }
            y = end;
        }
    }
    shape.0
}

/// Rows of a rounded rectangle, each with where it starts and ends.
#[derive(Clone, Copy)]
struct Row {
    y: i32,
    height: i32,
    left: i32,
    right: i32,
}

impl Row {
    fn bottom(&self) -> i32 {
        self.y + self.height
    }
}

fn sheet_rows(
    width: i32,
    height: i32,
    insets: WindowInsets,
    radius: i32,
) -> impl Iterator<Item = Row> {
    box_rows(
        insets.left,
        insets.top,
        width - insets.right,
        height - insets.bottom,
        radius,
    )
}

fn box_rows(
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
) -> impl Iterator<Item = Row> {
    let width = (right - left).max(0);
    let height = (bottom - top).max(0);
    let radius = radius.min(width / 2).min(height / 2).max(0);
    rounded_spans(0, height, height, radius)
        .map(move |span| Row {
            y: top + span.y,
            height: span.height,
            left: left + span.inset,
            right: right - span.inset,
        })
        .filter(move |_| width > 0)
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

/// Rects built row by row, top to bottom, with rows that line up with the
/// rect above them joined into it.
#[derive(Default)]
struct Shape(Vec<WindowRegionRect>);

impl Shape {
    fn add_span(&mut self, x: i32, span: Span, width: i32) {
        self.add(x, span.y, width, span.height);
    }

    fn add(&mut self, x: i32, y: i32, width: i32, height: i32) {
        let rect = WindowRegionRect::new(x, y, width, height);
        if rect.is_empty() {
            return;
        }
        let above = self.0.iter_mut().rev().find(|above| {
            above.x == rect.x && above.width == rect.width && above.y + above.height == rect.y
        });
        match above {
            Some(above) => above.height += rect.height,
            None => self.0.push(rect),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{WindowRegion, WindowRegions};

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

    fn bold(sidebar: i32) -> WindowRegions {
        let sheet = WindowInsets::new(sidebar, 8, 8, 8);
        WindowRegions::new()
            .blur(WindowRegion::around_inset(15, sheet, 7))
            .opaque(WindowRegion::inset(sheet, 7))
    }

    fn assert_tiles(regions: &WindowRegions, window: &WindowRegion, width: i32, height: i32) {
        let blur = regions.blur.as_ref().unwrap().resolved_rects(width, height);
        let opaque = regions
            .opaque
            .as_ref()
            .unwrap()
            .resolved_rects(width, height);
        let window = window.resolved_rects(width, height);
        for y in 0..height {
            for x in 0..width {
                let blurred = covers(&blur, x, y);
                let covered = covers(&opaque, x, y);
                assert!(!(blurred && covered), "{x},{y} is both glass and sheet");
                assert_eq!(
                    blurred || covered,
                    covers(&window, x, y),
                    "{x},{y} in a {width}x{height} window"
                );
            }
        }
    }

    #[test]
    fn sheet_and_its_frame_tile_the_window_at_every_size() {
        let regions = bold(280);
        let window = WindowRegion::rounded_rect(15);
        for (width, height) in [(900, 600), (480, 320), (281, 40)] {
            assert_tiles(&regions, &window, width, height);
        }
    }

    #[test]
    fn sheet_tracks_window_edges_at_fractional_scale() {
        let regions = bold(280);
        let window = WindowRegion::rounded_rect(15);
        for (physical_width, physical_height) in [(1001, 751), (1500, 1000), (1919, 1199)] {
            let width = (f64::from(physical_width) / 1.5).round() as i32;
            let height = (f64::from(physical_height) / 1.5).round() as i32;
            assert_tiles(&regions, &window, width, height);
            let opaque = regions
                .opaque
                .as_ref()
                .unwrap()
                .resolved_rects(width, height);
            assert!(covers(&opaque, width - 9, height / 2));
            assert!(!covers(&opaque, width - 8, height / 2));
            assert!(covers(&opaque, width / 2, height - 9));
            assert!(!covers(&opaque, width / 2, height - 8));
        }
    }

    #[test]
    fn sheet_corners_are_glass_and_window_corners_are_neither() {
        let regions = bold(280);
        let blur = regions.blur.as_ref().unwrap().resolved_rects(900, 600);
        let opaque = regions.opaque.as_ref().unwrap().resolved_rects(900, 600);
        assert!(!covers(&blur, 0, 0) && !covers(&opaque, 0, 0));
        assert!(covers(&blur, 280, 8) && !covers(&opaque, 280, 8));
        assert!(covers(&blur, 891, 591) && !covers(&opaque, 891, 591));
        assert!(covers(&opaque, 287, 8));
        assert!(covers(&opaque, 280, 15));
        assert!(covers(&blur, 120, 300));
        assert!(covers(&blur, 600, 4));
        assert!(!covers(&blur, 600, 300));
    }

    #[test]
    fn maximized_sheet_squares_with_the_window() {
        let regions = bold(280).squared();
        assert_tiles(&regions, &WindowRegion::full(), 1280, 720);
        let blur = regions.blur.as_ref().unwrap().resolved_rects(1280, 720);
        let opaque = regions.opaque.as_ref().unwrap().resolved_rects(1280, 720);
        assert!(covers(&blur, 0, 0));
        assert!(covers(&opaque, 280, 8));
        assert!(covers(&opaque, 1271, 711));
    }

    #[test]
    fn zoom_moves_the_sheet_but_keeps_window_corners() {
        let regions = bold(280).zoomed(1.5);
        assert_tiles(&regions, &WindowRegion::rounded_rect(15), 900, 600);
        let opaque = regions.opaque.as_ref().unwrap().resolved_rects(900, 600);
        assert!(covers(&opaque, 430, 300));
        assert!(!covers(&opaque, 419, 300));
        assert!(!covers(&opaque, 600, 11));
        assert!(!covers(&opaque, 430, 12));
        assert!(covers(&opaque, 431, 13));
    }

    #[test]
    fn frames_stay_a_handful_of_rects() {
        let blur = bold(280).blur.unwrap().resolved_rects(1600, 1000);
        assert!(blur.len() <= 15 * 2 + 7 * 4 + 4);
    }
}
