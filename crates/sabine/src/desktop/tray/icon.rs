use std::path::Path;

const FALLBACK_SIZE: u32 = 32;

pub(super) struct TrayImage {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// Decodes the icon at `path`. Without one, or when it cannot be read, the
/// tray shows a plain dot so the app stays reachable.
pub(super) fn load(path: Option<&Path>) -> TrayImage {
    let Some(path) = path else {
        return fallback();
    };
    match image::open(path) {
        Ok(image) => {
            let image = image.into_rgba8();
            let (width, height) = image.dimensions();
            TrayImage {
                rgba: image.into_raw(),
                width,
                height,
            }
        }
        Err(error) => {
            sabine_runtime::report_error(
                "desktop",
                format!("could not read the tray icon {}: {error}", path.display()),
            );
            fallback()
        }
    }
}

impl TrayImage {
    /// Whether every visible pixel is black, or every one white: an icon the
    /// macOS menu bar can tint as a template.
    pub(super) fn is_black_and_white(&self) -> bool {
        let mut visible = self
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|[.., alpha]| *alpha > 0)
            .map(|&[red, green, blue, _]| (red, green, blue))
            .peekable();
        let Some(&(red, _, _)) = visible.peek() else {
            return false;
        };
        let dark = red < 128;
        visible.all(|(red, green, blue)| {
            let channels = [red, green, blue];
            if dark {
                channels.iter().all(|channel| *channel <= 24)
            } else {
                channels.iter().all(|channel| *channel >= 232)
            }
        })
    }

    pub(super) fn into_icon(self) -> Result<tray_icon::Icon, String> {
        tray_icon::Icon::from_rgba(self.rgba, self.width, self.height)
            .map_err(|error| error.to_string())
    }
}

fn fallback() -> TrayImage {
    let center = FALLBACK_SIZE as f32 / 2.0;
    let radius = center - 4.0;
    let mut rgba = Vec::with_capacity((FALLBACK_SIZE * FALLBACK_SIZE * 4) as usize);
    for y in 0..FALLBACK_SIZE {
        for x in 0..FALLBACK_SIZE {
            let distance = (x as f32 + 0.5 - center).hypot(y as f32 + 0.5 - center);
            let coverage = (radius - distance + 0.5).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[0x8E, 0x8E, 0x93, (coverage * 255.0) as u8]);
        }
    }
    TrayImage {
        rgba,
        width: FALLBACK_SIZE,
        height: FALLBACK_SIZE,
    }
}
