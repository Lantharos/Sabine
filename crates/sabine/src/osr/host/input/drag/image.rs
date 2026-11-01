use winit::{event_loop::DragIcon, icon::RgbaIcon};

use crate::osr::protocol::DragImage;

/// The picture the desktop shows under the pointer while it carries a page
/// drag, with the pointer where it held the dragged content. The desktop
/// shows it at `desktop_scale` pixels per page pixel.
pub(super) fn drag_icon(image: DragImage, desktop_scale: f32) -> Option<DragIcon> {
    let image = shrunk_to(image, desktop_scale);
    let (offset_x, offset_y) = (-image.hotspot_x, -image.hotspot_y);
    let icon = RgbaIcon::new(image.rgba, image.width, image.height).ok()?;
    Some(DragIcon {
        icon: icon.into(),
        offset_x,
        offset_y,
    })
}

/// Windows shows drag pictures in screen pixels, and Wayland and macOS in
/// points.
pub(super) fn desktop_scale(window_scale: f64) -> f32 {
    if cfg!(windows) {
        window_scale as f32
    } else {
        1.0
    }
}

fn shrunk_to(image: DragImage, scale: f32) -> DragImage {
    let ratio = image.scale / scale;
    if ratio <= 1.0 {
        return image;
    }
    let width = ((image.width as f32 / ratio).round() as u32).max(1);
    let height = ((image.height as f32 / ratio).round() as u32).max(1);
    DragImage {
        rgba: shrink(&image, width, height),
        width,
        height,
        hotspot_x: (image.hotspot_x as f32 / ratio).round() as i32,
        hotspot_y: (image.hotspot_y as f32 / ratio).round() as i32,
        scale,
    }
}

/// Averages each area of the picture into one pixel, weighing colors by how
/// opaque they are so translucent edges keep their color.
fn shrink(image: &DragImage, width: u32, height: u32) -> Vec<u8> {
    let source_width = image.width as usize;
    let span = |target: u32, target_len: u32, source_len: u32| {
        let start = target as usize * source_len as usize / target_len as usize;
        let end = (target as usize + 1) * source_len as usize / target_len as usize;
        start..end.max(start + 1)
    };
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        let rows = span(y, height, image.height);
        for x in 0..width {
            let columns = span(x, width, image.width);
            let mut sum = [0_u64; 4];
            let mut count = 0_u64;
            for row in rows.clone() {
                for column in columns.clone() {
                    let index = (row * source_width + column) * 4;
                    let pixel = &image.rgba[index..index + 4];
                    let alpha = u64::from(pixel[3]);
                    for (total, channel) in sum.iter_mut().zip(&pixel[..3]) {
                        *total += u64::from(*channel) * alpha;
                    }
                    sum[3] += alpha;
                    count += 1;
                }
            }
            let alpha = sum[3];
            for total in &sum[..3] {
                rgba.push(total.checked_div(alpha).unwrap_or(0) as u8);
            }
            rgba.push((alpha / count) as u8);
        }
    }
    rgba
}
