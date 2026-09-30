use crate::render::PixelRect;

/// BGRA pixels to write into `target` of a texture, read from `bytes`
/// starting at `offset` with rows `bytes_per_row` apart.
pub(crate) struct BgraRect<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) offset: u64,
    pub(crate) bytes_per_row: u32,
    pub(crate) target: PixelRect,
}

/// A tightly packed BGRA image kept on the CPU while no renderer holds it.
pub(crate) struct BgraImage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bytes: Vec<u8>,
}

impl BgraImage {
    pub(crate) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn whole(&self) -> BgraRect<'_> {
        BgraRect {
            bytes: &self.bytes,
            offset: 0,
            bytes_per_row: self.width * 4,
            target: PixelRect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
        }
    }
}
