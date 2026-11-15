//! Pictures a page puts on its notifications arrive as `data:` URLs, read by
//! the page from wherever it loads them, and the desktop shows pictures from
//! files. Each one is kept once in the app's runtime folder, named after its
//! contents, which the desktop clears when the person logs out.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use base64::{Engine, engine::general_purpose::STANDARD};

const FOLDER: &str = "notifications";

/// The file showing a page's picture, or `None` for anything but a
/// picture in a format desktops show.
pub(super) fn picture_file(app_id: &str, data_url: &str) -> Option<PathBuf> {
    let (header, data) = data_url.strip_prefix("data:")?.split_once(',')?;
    let extension = extension(header.strip_suffix(";base64")?)?;
    let bytes = STANDARD.decode(data).ok()?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    let folder = crate::osr::transport::runtime_ipc_dir(app_id)
        .ok()?
        .join(FOLDER);
    let file = folder.join(format!("{:016x}.{extension}", hasher.finish()));
    if !file.exists() {
        std::fs::create_dir_all(&folder).ok()?;
        std::fs::write(&file, bytes).ok()?;
    }
    Some(file)
}

fn extension(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/svg+xml" => "svg",
        _ => return None,
    })
}
