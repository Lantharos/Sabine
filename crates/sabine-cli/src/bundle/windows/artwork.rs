use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat, RgbImage, imageops};

use crate::desktop::icons::load_raster;

const NSIS_WELCOME: (u32, u32) = (164, 314);
const WIX_DIALOG: (u32, u32) = (493, 312);
const WIX_ARTWORK_WIDTH: u32 = 164;

/// Modern UI's welcome and finish pages show this bitmap down their left side.
pub(in crate::bundle) fn nsis_welcome(image: &Path, root: &Path) -> Result<PathBuf, String> {
    let (width, height) = NSIS_WELCOME;
    let bitmap = fill(&load_raster(image)?, width, height);
    write_bitmap(bitmap, &root.join("installer-welcome.bmp"))
}

/// WixUI stretches this bitmap behind its welcome and finish dialogs, with the
/// text on the white area to the right of the artwork.
pub(in crate::bundle) fn wix_dialog(image: &Path, root: &Path) -> Result<PathBuf, String> {
    let (width, height) = WIX_DIALOG;
    let mut bitmap = RgbImage::from_pixel(width, height, image::Rgb([255, 255, 255]));
    imageops::replace(
        &mut bitmap,
        &fill(&load_raster(image)?, WIX_ARTWORK_WIDTH, height),
        0,
        0,
    );
    write_bitmap(bitmap, &root.join("installer-dialog.bmp"))
}

fn fill(image: &DynamicImage, width: u32, height: u32) -> RgbImage {
    image
        .resize_to_fill(width, height, imageops::FilterType::Lanczos3)
        .into_rgb8()
}

fn write_bitmap(bitmap: RgbImage, destination: &Path) -> Result<PathBuf, String> {
    bitmap
        .save_with_format(destination, ImageFormat::Bmp)
        .map_err(|error| format!("failed to write {}: {error}", destination.display()))?;
    Ok(destination.to_path_buf())
}
