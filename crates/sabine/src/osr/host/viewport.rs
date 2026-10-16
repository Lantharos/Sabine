/// The page Chromium renders, in DIPs, at the scale Chromium gets for it.
///
/// Chromium keeps the device scale factor in single precision and sizes the
/// view's pixels with `gfx::ScaleToCeiledSize`, so the page is
/// `ceil(dips * scale)` pixels wide, computed in `f32`. A window `1180` DIPs
/// wide at 4/3 is `1574` pixels, not the `1573` a rounded conversion gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::osr) struct PageViewport {
    pub(in crate::osr) width: u32,
    pub(in crate::osr) height: u32,
    pub(in crate::osr) scale: f32,
}

impl PageViewport {
    pub(in crate::osr) fn new(width: u32, height: u32, scale: f64) -> Self {
        Self {
            width,
            height,
            scale: scale as f32,
        }
    }

    /// The size of the frames Chromium paints for this page.
    pub(in crate::osr) fn pixel_size(self) -> (u32, u32) {
        (
            chromium_pixels(self.width, self.scale),
            chromium_pixels(self.height, self.scale),
        )
    }

    /// The scale written so the browser parses back exactly `self.scale`.
    pub(in crate::osr) fn scale_argument(self) -> String {
        self.scale.to_string()
    }
}

fn chromium_pixels(dips: u32, scale: f32) -> u32 {
    (dips as f32 * scale).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::PageViewport;

    fn pixels(width: u32, height: u32, scale: f64) -> (u32, u32) {
        PageViewport::new(width, height, scale).pixel_size()
    }

    #[test]
    fn rounds_fractional_pixels_up_like_chromium() {
        assert_eq!(pixels(1180, 780, 4.0 / 3.0), (1574, 1040));
        assert_eq!(pixels(1181, 777, 4.0 / 3.0), (1575, 1036));
        assert_eq!(pixels(1181, 777, 1.25), (1477, 972));
        assert_eq!(pixels(1181, 777, 1.75), (2067, 1360));
    }

    #[test]
    fn rounds_in_single_precision_like_chromium() {
        assert_eq!(pixels(1179, 780, 4.0 / 3.0), (1572, 1040));
        assert_eq!(pixels(25, 50, 1.2), (31, 61));
    }
}
