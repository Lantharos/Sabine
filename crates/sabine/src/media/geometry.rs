use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl Rect {
    pub(crate) fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    fn right(self) -> f64 {
        self.x + self.width
    }

    fn bottom(self) -> f64 {
        self.y + self.height
    }

    fn offset(self, x: f64, y: f64) -> Self {
        Self::new(self.x + x, self.y + y, self.width, self.height)
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let width = self.right().min(other.right()) - x;
        let height = self.bottom().min(other.bottom()) - y;
        (width > 0.0 && height > 0.0).then(|| Self::new(x, y, width, height))
    }
}

/// Where a page wants its media drawn, in page CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PageRect {
    pub(super) bounds: Rect,
    pub(super) clip: Option<Rect>,
    pub(super) radius: f64,
}

/// Window area that shows a media surface through the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MediaHole {
    pub(crate) bounds: Rect,
    pub(crate) radius: f64,
    pub(crate) surface: SurfaceRect,
}

/// A rectangle in whole window-surface units, as Wayland positions surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceRect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

/// How a player lays its video into its surface buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Frame {
    pub(super) buffer: (i32, i32),
    pub(super) destination: (i32, i32),
    pub(super) video: [f32; 4],
    pub(super) radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub(super) hole: MediaHole,
    pub(super) frame: Frame,
}

/// The page content area inside the window, and the window's scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Viewport {
    pub(crate) content: Rect,
    pub(crate) scale: f64,
}

impl PageRect {
    pub(super) fn layout(self, viewport: Viewport) -> Option<Layout> {
        let content = viewport.content;
        let bounds = self.bounds.offset(content.x, content.y);
        let clip = match self.clip {
            Some(clip) => clip.offset(content.x, content.y).intersect(content)?,
            None => content,
        };
        let visible = bounds.intersect(clip)?;
        let surface = SurfaceRect::around(visible)?;
        let scale = viewport.scale;
        let unclipped = visible == bounds;
        Some(Layout {
            hole: MediaHole {
                bounds: visible,
                radius: if unclipped { self.radius } else { 0.0 },
                surface,
            },
            frame: Frame {
                buffer: (
                    (f64::from(surface.width) * scale).round() as i32,
                    (f64::from(surface.height) * scale).round() as i32,
                ),
                destination: (surface.width, surface.height),
                video: [
                    ((bounds.x - f64::from(surface.x)) * scale) as f32,
                    ((bounds.y - f64::from(surface.y)) * scale) as f32,
                    (bounds.width * scale) as f32,
                    (bounds.height * scale) as f32,
                ],
                radius: (self.radius * scale) as f32,
            },
        })
    }
}

impl SurfaceRect {
    fn around(rect: Rect) -> Option<Self> {
        let x = rect.x.round() as i32;
        let y = rect.y.round() as i32;
        let width = rect.right().round() as i32 - x;
        let height = rect.bottom().round() as i32 - y;
        (width > 0 && height > 0).then_some(Self {
            x,
            y,
            width,
            height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Viewport = Viewport {
        content: Rect {
            x: 0.0,
            y: 38.0,
            width: 800.0,
            height: 562.0,
        },
        scale: 1.5,
    };

    #[test]
    fn places_unclipped_media_below_the_titlebar() {
        let layout = PageRect {
            bounds: Rect::new(10.0, 20.0, 400.0, 225.0),
            clip: None,
            radius: 8.0,
        }
        .layout(VIEWPORT)
        .unwrap();
        assert_eq!(
            layout.hole.surface,
            SurfaceRect {
                x: 10,
                y: 58,
                width: 400,
                height: 225
            }
        );
        assert_eq!(layout.hole.radius, 8.0);
        assert_eq!(layout.frame.buffer, (600, 338));
        assert_eq!(layout.frame.video, [0.0, 0.0, 600.0, 337.5]);
    }

    #[test]
    fn crops_media_scrolled_past_its_clip() {
        let layout = PageRect {
            bounds: Rect::new(0.0, -100.0, 400.0, 300.0),
            clip: Some(Rect::new(0.0, 0.0, 800.0, 400.0)),
            radius: 8.0,
        }
        .layout(VIEWPORT)
        .unwrap();
        assert_eq!(layout.hole.surface.y, 38);
        assert_eq!(layout.hole.surface.height, 200);
        assert_eq!(layout.hole.radius, 0.0);
        assert_eq!(layout.frame.video[1], -150.0);
    }

    #[test]
    fn hides_media_outside_the_page() {
        let hidden = PageRect {
            bounds: Rect::new(900.0, 0.0, 100.0, 100.0),
            clip: None,
            radius: 0.0,
        };
        assert!(hidden.layout(VIEWPORT).is_none());
    }
}
