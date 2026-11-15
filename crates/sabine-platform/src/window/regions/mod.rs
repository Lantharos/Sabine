mod shapes;

use shapes::{
    content_after_sidebar, content_after_sidebar_rounded_right, rounded_left, rounded_rect,
    titlebar_and_sidebar,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowRegionRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl WindowRegionRect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width: width.max(0),
            height: height.max(0),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    /// The rect as a page zoomed by `zoom` lays it out: its edges move
    /// `zoom` times as far from the window's left and top, or from its right
    /// for rects placed from there.
    pub fn zoomed(&self, zoom: f64) -> Self {
        let scale = |value: i32| (f64::from(value) * zoom).round() as i32;
        let x = scale(self.x);
        let y = scale(self.y);
        Self {
            x,
            y,
            width: scale(self.x.saturating_add(self.width)) - x,
            height: scale(self.y.saturating_add(self.height)) - y,
        }
    }
}

/// A part of the window, in logical pixels. Its shapes follow the window's
/// size as it changes; rectangles from [`WindowRegion::rect`] stay put.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowRegion {
    pub rects: Vec<WindowRegionRect>,
    pub adaptive: Option<WindowRegionAdaptive>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowRegionAdaptive {
    Full,
    RoundedRect {
        radius: i32,
    },
    RoundedLeft {
        width: i32,
        radius: i32,
    },
    TitlebarAndSidebar {
        sidebar_width: i32,
        titlebar_height: i32,
        radius: i32,
    },
    ContentAfterSidebar {
        sidebar_width: i32,
        titlebar_height: i32,
    },
    ContentAfterSidebarRoundedRight {
        sidebar_width: i32,
        titlebar_height: i32,
        radius: i32,
    },
}

impl WindowRegion {
    pub fn empty() -> Self {
        Self {
            rects: Vec::new(),
            adaptive: None,
        }
    }

    pub fn rect(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self::empty().add_rect(x, y, width, height)
    }

    /// The whole window.
    pub fn full() -> Self {
        Self::adaptive(WindowRegionAdaptive::Full)
    }

    /// The whole window with rounded corners.
    pub fn rounded_rect(radius: i32) -> Self {
        Self::adaptive(WindowRegionAdaptive::RoundedRect { radius })
    }

    /// A full-height column along the left edge, rounded on its left corners.
    pub fn rounded_left(width: i32, radius: i32) -> Self {
        Self::adaptive(WindowRegionAdaptive::RoundedLeft { width, radius })
    }

    /// A titlebar strip across the top and a sidebar column below it.
    pub fn titlebar_and_sidebar(sidebar_width: i32, titlebar_height: i32, radius: i32) -> Self {
        Self::adaptive(WindowRegionAdaptive::TitlebarAndSidebar {
            sidebar_width,
            titlebar_height,
            radius,
        })
    }

    /// Everything right of the sidebar and below the titlebar.
    pub fn content_after_sidebar(sidebar_width: i32, titlebar_height: i32) -> Self {
        Self::adaptive(WindowRegionAdaptive::ContentAfterSidebar {
            sidebar_width,
            titlebar_height,
        })
    }

    /// Everything right of the sidebar and below the titlebar, rounded on its
    /// right corners.
    pub fn content_after_sidebar_rounded_right(
        sidebar_width: i32,
        titlebar_height: i32,
        radius: i32,
    ) -> Self {
        Self::adaptive(WindowRegionAdaptive::ContentAfterSidebarRoundedRight {
            sidebar_width,
            titlebar_height,
            radius,
        })
    }

    fn adaptive(adaptive: WindowRegionAdaptive) -> Self {
        Self {
            rects: Vec::new(),
            adaptive: Some(adaptive),
        }
    }

    /// The region as a page zoomed by `zoom` lays it out. Corner radii belong
    /// to the window, not the page, and stay as they are.
    pub fn zoomed(&self, zoom: f64) -> Self {
        let scale = |value: i32| (f64::from(value) * zoom).round() as i32;
        let adaptive = self.adaptive.clone().map(|adaptive| match adaptive {
            WindowRegionAdaptive::RoundedLeft { width, radius } => {
                WindowRegionAdaptive::RoundedLeft {
                    width: scale(width),
                    radius,
                }
            }
            WindowRegionAdaptive::TitlebarAndSidebar {
                sidebar_width,
                titlebar_height,
                radius,
            } => WindowRegionAdaptive::TitlebarAndSidebar {
                sidebar_width: scale(sidebar_width),
                titlebar_height: scale(titlebar_height),
                radius,
            },
            WindowRegionAdaptive::ContentAfterSidebar {
                sidebar_width,
                titlebar_height,
            } => WindowRegionAdaptive::ContentAfterSidebar {
                sidebar_width: scale(sidebar_width),
                titlebar_height: scale(titlebar_height),
            },
            WindowRegionAdaptive::ContentAfterSidebarRoundedRight {
                sidebar_width,
                titlebar_height,
                radius,
            } => WindowRegionAdaptive::ContentAfterSidebarRoundedRight {
                sidebar_width: scale(sidebar_width),
                titlebar_height: scale(titlebar_height),
                radius,
            },
            whole_window => whole_window,
        });
        Self {
            rects: self.rects.iter().map(|rect| rect.zoomed(zoom)).collect(),
            adaptive,
        }
    }

    /// The same region with square corners, for windows against screen edges.
    pub fn squared(&self) -> Self {
        let adaptive = self.adaptive.clone().map(|adaptive| match adaptive {
            WindowRegionAdaptive::RoundedRect { .. } => WindowRegionAdaptive::Full,
            WindowRegionAdaptive::RoundedLeft { width, .. } => {
                WindowRegionAdaptive::RoundedLeft { width, radius: 0 }
            }
            WindowRegionAdaptive::TitlebarAndSidebar {
                sidebar_width,
                titlebar_height,
                ..
            } => WindowRegionAdaptive::TitlebarAndSidebar {
                sidebar_width,
                titlebar_height,
                radius: 0,
            },
            WindowRegionAdaptive::ContentAfterSidebarRoundedRight {
                sidebar_width,
                titlebar_height,
                ..
            } => WindowRegionAdaptive::ContentAfterSidebar {
                sidebar_width,
                titlebar_height,
            },
            square => square,
        });
        Self {
            rects: self.rects.clone(),
            adaptive,
        }
    }

    pub fn add_rect(mut self, x: i32, y: i32, width: i32, height: i32) -> Self {
        let rect = WindowRegionRect::new(x, y, width, height);
        if !rect.is_empty() {
            self.rects.push(rect);
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty() && self.adaptive.is_none()
    }

    pub fn resolved_rects(&self, width: i32, height: i32) -> Vec<WindowRegionRect> {
        match self.adaptive {
            Some(WindowRegionAdaptive::Full) => vec![WindowRegionRect::new(0, 0, width, height)],
            Some(WindowRegionAdaptive::RoundedRect { radius }) => {
                rounded_rect(width, height, radius)
            }
            Some(WindowRegionAdaptive::RoundedLeft {
                width: sidebar_width,
                radius,
            }) => rounded_left(sidebar_width, height, radius),
            Some(WindowRegionAdaptive::TitlebarAndSidebar {
                sidebar_width,
                titlebar_height,
                radius,
            }) => titlebar_and_sidebar(width, height, sidebar_width, titlebar_height, radius),
            Some(WindowRegionAdaptive::ContentAfterSidebar {
                sidebar_width,
                titlebar_height,
            }) => content_after_sidebar(width, height, sidebar_width, titlebar_height),
            Some(WindowRegionAdaptive::ContentAfterSidebarRoundedRight {
                sidebar_width,
                titlebar_height,
                radius,
            }) => content_after_sidebar_rounded_right(
                width,
                height,
                sidebar_width,
                titlebar_height,
                radius,
            ),
            None => self.rects.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowRegions {
    pub blur: Option<WindowRegion>,
    pub opaque: Option<WindowRegion>,
    pub input: Option<WindowRegion>,
}

impl WindowRegions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn blur(mut self, region: WindowRegion) -> Self {
        self.blur = Some(region);
        self
    }

    pub fn opaque(mut self, region: WindowRegion) -> Self {
        self.opaque = Some(region);
        self
    }

    pub fn input(mut self, region: WindowRegion) -> Self {
        self.input = Some(region);
        self
    }

    /// The regions as a page zoomed by `zoom` lays them out.
    pub fn zoomed(&self, zoom: f64) -> Self {
        let zoomed = |region: &WindowRegion| region.zoomed(zoom);
        Self {
            blur: self.blur.as_ref().map(zoomed),
            opaque: self.opaque.as_ref().map(zoomed),
            input: self.input.as_ref().map(zoomed),
        }
    }

    /// The same regions with square corners, for windows against screen edges.
    pub fn squared(&self) -> Self {
        Self {
            blur: self.blur.as_ref().map(WindowRegion::squared),
            opaque: self.opaque.as_ref().map(WindowRegion::squared),
            input: self.input.as_ref().map(WindowRegion::squared),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.blur.as_ref().is_none_or(WindowRegion::is_empty)
            && self.opaque.as_ref().is_none_or(WindowRegion::is_empty)
            && self.input.as_ref().is_none_or(WindowRegion::is_empty)
    }
}
