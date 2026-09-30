use crate::osr::protocol::PaintRect;
use crate::render::{BgraRect, PixelRect};

/// Crops each painted rect to the `width` x `height` surface. Returns `None`
/// when nothing lands on the surface or any rect's pixels are truncated, so a
/// broken paint never reaches the surface.
pub(crate) fn surface_rects(
    rects: &[PaintRect],
    width: u32,
    height: u32,
) -> Option<Vec<BgraRect<'_>>> {
    let rects = rects
        .iter()
        .map(|rect| surface_rect(rect, width, height))
        .collect::<Option<Vec<_>>>()?;
    (!rects.is_empty()).then_some(rects)
}

fn surface_rect(rect: &PaintRect, width: u32, height: u32) -> Option<BgraRect<'_>> {
    let left = i64::from(rect.x).max(0);
    let top = i64::from(rect.y).max(0);
    let right = (i64::from(rect.x) + i64::from(rect.width)).min(i64::from(width));
    let bottom = (i64::from(rect.y) + i64::from(rect.height)).min(i64::from(height));
    if right <= left || bottom <= top {
        return None;
    }
    let target = PixelRect {
        x: left as u32,
        y: top as u32,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    };
    let source_x = u64::from(rect.x.min(0).unsigned_abs());
    let source_y = u64::from(rect.y.min(0).unsigned_abs());
    let offset = (source_y * u64::from(rect.width) + source_x) * 4;
    let last_row_end = ((source_y + u64::from(target.height) - 1) * u64::from(rect.width)
        + source_x
        + u64::from(target.width))
        * 4;
    let bytes = rect.bytes();
    (last_row_end <= bytes.len() as u64).then_some(BgraRect {
        bytes,
        offset,
        bytes_per_row: rect.width * 4,
        target,
    })
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
    fn dirty_rect_targets_its_offset() {
        let dirty = rect(1, 0, 1, 1, vec![9, 9, 9, 255]);
        let rects = surface_rects(std::slice::from_ref(&dirty), 3, 2).expect("dirty rect");

        assert_eq!(
            rects[0].target,
            PixelRect {
                x: 1,
                y: 0,
                width: 1,
                height: 1
            }
        );
        assert_eq!((rects[0].offset, rects[0].bytes_per_row), (0, 4));
    }

    #[test]
    fn negative_dirty_rect_is_cropped_into_target() {
        let frame = rect(
            -1,
            -1,
            2,
            2,
            vec![1, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255, 4, 4, 4, 255],
        );
        let rects = surface_rects(std::slice::from_ref(&frame), 2, 2).expect("cropped rect");

        assert_eq!(
            rects[0].target,
            PixelRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            }
        );
        let start = rects[0].offset as usize;
        assert_eq!(&rects[0].bytes[start..start + 4], &[4, 4, 4, 255]);
    }

    #[test]
    fn truncated_rect_rejects_the_whole_paint() {
        let valid = rect(0, 0, 1, 1, vec![8, 8, 8, 255]);
        let truncated = rect(1, 0, 1, 1, vec![9, 9, 9]);

        assert!(surface_rects(&[valid, truncated], 2, 1).is_none());
    }
}
