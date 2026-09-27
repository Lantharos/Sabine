use crate::osr::protocol::PaintRect;
use crate::render::PixelRect;

pub(crate) struct FrameBuffer {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

struct Blit {
    source_x: u32,
    source_y: u32,
    target: PixelRect,
}

impl FrameBuffer {
    pub(crate) fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            bytes: Vec::new(),
        }
    }

    pub(crate) fn release(&mut self) {
        self.width = 0;
        self.height = 0;
        self.bytes = Vec::new();
    }

    pub(crate) fn compose(
        &mut self,
        width: u32,
        height: u32,
        rects: &[PaintRect],
    ) -> Option<Vec<PixelRect>> {
        let blits = rects
            .iter()
            .map(|rect| blit(rect, width, height))
            .collect::<Option<Vec<_>>>()?;
        if blits.is_empty() {
            return None;
        }
        self.ensure_size(width, height);
        let target_stride = width as usize * 4;
        for (rect, blit) in rects.iter().zip(&blits) {
            let source_stride = rect.width as usize * 4;
            let row_bytes = blit.target.width as usize * 4;
            let source = rect.bytes();
            for row in 0..blit.target.height as usize {
                let source_start =
                    (blit.source_y as usize + row) * source_stride + blit.source_x as usize * 4;
                let target_start =
                    (blit.target.y as usize + row) * target_stride + blit.target.x as usize * 4;
                self.bytes[target_start..target_start + row_bytes]
                    .copy_from_slice(&source[source_start..source_start + row_bytes]);
            }
        }
        Some(blits.into_iter().map(|blit| blit.target).collect())
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn ensure_size(&mut self, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        self.width = width;
        self.height = height;
        self.bytes.clear();
        self.bytes.resize(width as usize * height as usize * 4, 0);
    }
}

fn blit(rect: &PaintRect, width: u32, height: u32) -> Option<Blit> {
    let left = i64::from(rect.x).max(0);
    let top = i64::from(rect.y).max(0);
    let right = (i64::from(rect.x) + i64::from(rect.width)).min(i64::from(width));
    let bottom = (i64::from(rect.y) + i64::from(rect.height)).min(i64::from(height));
    if right <= left || bottom <= top {
        return None;
    }
    let blit = Blit {
        source_x: rect.x.min(0).unsigned_abs(),
        source_y: rect.y.min(0).unsigned_abs(),
        target: PixelRect {
            x: left as u32,
            y: top as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        },
    };
    let last_row_end = ((u64::from(blit.source_y) + u64::from(blit.target.height) - 1)
        * u64::from(rect.width)
        + u64::from(blit.source_x)
        + u64::from(blit.target.width))
        * 4;
    (last_row_end <= rect.bytes().len() as u64).then_some(blit)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::osr::protocol::FrameBytes;

    fn rect(x: i32, y: i32, width: u32, height: u32, bytes: Vec<u8>) -> PaintRect {
        let len = bytes.len();
        PaintRect {
            width,
            height,
            x,
            y,
            bytes: FrameBytes::Inline {
                source: Arc::from(bytes),
                range: 0..len,
            },
        }
    }

    #[test]
    fn dirty_rect_patches_backing_store_at_offset() {
        let mut buffer = FrameBuffer::new();
        let full = rect(
            0,
            0,
            3,
            2,
            vec![
                1, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255, 4, 4, 4, 255, 5, 5, 5, 255, 6, 6, 6, 255,
            ],
        );
        let dirty = rect(1, 0, 1, 1, vec![9, 9, 9, 255]);

        buffer.compose(3, 2, &[full]).expect("full frame");
        assert_eq!(
            buffer.compose(3, 2, &[dirty]),
            Some(vec![PixelRect {
                x: 1,
                y: 0,
                width: 1,
                height: 1
            }])
        );

        assert_eq!(&buffer.bytes()[0..4], &[1, 1, 1, 255]);
        assert_eq!(&buffer.bytes()[4..8], &[9, 9, 9, 255]);
        assert_eq!(&buffer.bytes()[8..12], &[3, 3, 3, 255]);
        assert_eq!(&buffer.bytes()[20..24], &[6, 6, 6, 255]);
    }

    #[test]
    fn negative_dirty_rect_is_cropped_into_target() {
        let mut buffer = FrameBuffer::new();
        let frame = rect(
            -1,
            -1,
            2,
            2,
            vec![1, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255, 4, 4, 4, 255],
        );

        assert_eq!(
            buffer.compose(2, 2, &[frame]),
            Some(vec![PixelRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }])
        );
        assert_eq!(&buffer.bytes()[0..4], &[4, 4, 4, 255]);
        assert_eq!(&buffer.bytes()[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn invalid_dirty_payload_does_not_update_backing_store() {
        let mut buffer = FrameBuffer::new();
        let full = rect(0, 0, 2, 1, vec![7, 7, 7, 255, 7, 7, 7, 255]);
        let valid = rect(0, 0, 1, 1, vec![8, 8, 8, 255]);
        let truncated = rect(1, 0, 1, 1, vec![9, 9, 9]);

        buffer.compose(2, 1, &[full]).expect("full frame");
        assert!(buffer.compose(2, 1, &[valid, truncated]).is_none());
        assert_eq!(&buffer.bytes()[0..8], &[7, 7, 7, 255, 7, 7, 7, 255]);
    }
}
