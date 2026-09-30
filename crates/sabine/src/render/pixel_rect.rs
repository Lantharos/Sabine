#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PixelRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl PixelRect {
    pub(crate) fn fits(self, width: u32, height: u32) -> bool {
        self.x
            .checked_add(self.width)
            .is_some_and(|right| right <= width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|bottom| bottom <= height)
    }
}
