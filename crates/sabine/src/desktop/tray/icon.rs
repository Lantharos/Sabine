use std::{
    borrow::Cow,
    fmt,
    fs::OpenOptions,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
};

use image::{DynamicImage, ImageReader, Limits, imageops::FilterType};

const FALLBACK_SIZE: u32 = 32;
/// Far more than any tray image takes, as a file or as decoded pixels.
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DECODED_SIDE: u32 = 4096;
/// The most pixels across trays show an icon with, even on dense screens.
const TRAY_SIZE: u32 = 128;

/// Where a tray image comes from: a file the app names, or bytes a page
/// sends.
pub(crate) enum IconSource {
    File(PathBuf),
    Bytes(Vec<u8>),
}

impl fmt::Display for IconSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => path.display().fmt(formatter),
            Self::Bytes(_) => formatter.write_str("from the page"),
        }
    }
}

pub(super) struct TrayImage {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// Decodes the icon, at most as large as trays show it. Without one, or
/// when it cannot be read, the tray shows a plain dot so the app stays
/// reachable.
pub(super) fn load(source: Option<&IconSource>) -> TrayImage {
    let Some(source) = source else {
        return fallback();
    };
    match decode(source) {
        Ok(image) => {
            let image = if image.width() > TRAY_SIZE || image.height() > TRAY_SIZE {
                image.resize(TRAY_SIZE, TRAY_SIZE, FilterType::Lanczos3)
            } else {
                image
            };
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
                format!("could not read the tray icon {source}: {error}"),
            );
            fallback()
        }
    }
}

fn decode(source: &IconSource) -> Result<DynamicImage, String> {
    let bytes = match source {
        IconSource::File(path) => Cow::Owned(read_file(path).map_err(|error| error.to_string())?),
        IconSource::Bytes(bytes) => Cow::Borrowed(bytes.as_slice()),
    };
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("the image is larger than 8 MiB".to_string());
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    limits.max_alloc = Some(MAX_FILE_BYTES * 8);
    reader.limits(limits);
    reader.decode().map_err(|error| error.to_string())
}

/// Reads at most one byte past the limit of a regular file. It opens without
/// waiting, so a name that leads to a pipe or device is turned down instead
/// of holding up the tray.
fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NONBLOCK);
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "it is not a file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

impl TrayImage {
    /// Whether every visible pixel is black, or every one white: an icon the
    /// macOS menu bar can tint as a template.
    #[cfg(target_os = "macos")]
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

    #[cfg(not(target_os = "linux"))]
    pub(super) fn into_icon(self) -> Result<tray_icon::Icon, String> {
        tray_icon::Icon::from_rgba(self.rgba, self.width, self.height)
            .map_err(|error| error.to_string())
    }

    /// The image as a StatusNotifierItem pixmap: its size and ARGB pixels in
    /// network byte order.
    #[cfg(target_os = "linux")]
    pub(super) fn into_pixmap(self) -> (i32, i32, Vec<u8>) {
        let mut argb = self.rgba;
        for pixel in argb.as_chunks_mut::<4>().0 {
            pixel.rotate_right(1);
        }
        (self.width as i32, self.height as i32, argb)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_images_shrink_to_what_trays_show() {
        let mut png = Vec::new();
        image::RgbaImage::new(1024, 512)
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("png");
        let image = load(Some(&IconSource::Bytes(png)));
        assert_eq!((image.width, image.height), (TRAY_SIZE, TRAY_SIZE / 2));
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_is_turned_down_without_waiting_for_a_writer() {
        let pipe = std::env::temp_dir().join(format!("sabine-tray-{}", std::process::id()));
        let name = std::ffi::CString::new(pipe.as_os_str().as_encoded_bytes()).expect("path");
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let decoded = decode(&IconSource::File(pipe.clone()));
        let _ = std::fs::remove_file(&pipe);
        assert!(decoded.is_err());
    }
}
