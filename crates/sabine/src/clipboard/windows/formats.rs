// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Clipboard formats are Windows' own: UTF-16 text, the "HTML Format" header
// with byte offsets into UTF-8, DROPFILES lists for files, and device
// independent bitmaps that most apps read instead of PNG.

use std::io::Cursor;

use windows::Win32::System::{
    DataExchange::{GetClipboardFormatNameW, RegisterClipboardFormatW},
    Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT},
};
use windows::core::HSTRING;

const HTML_FORMAT: &str = "HTML Format";
const PNG_FORMAT: &str = "PNG";
const BITMAP_FILE_HEADER: usize = 14;
const BITMAP_INFO_HEADER: u32 = 40;

pub(super) enum Format {
    Text,
    Html(u32),
    Png(u32),
    Bitmap,
    Files,
    Named { id: u32, mime: String },
}

impl Format {
    pub(super) fn from_id(id: u32) -> Option<Self> {
        match id {
            id if id == u32::from(CF_UNICODETEXT.0) => Some(Self::Text),
            id if id == u32::from(CF_DIB.0) => Some(Self::Bitmap),
            id if id == u32::from(CF_HDROP.0) => Some(Self::Files),
            id => match format_name(id)?.as_str() {
                HTML_FORMAT => Some(Self::Html(id)),
                PNG_FORMAT => Some(Self::Png(id)),
                name if name.contains('/') => Some(Self::Named {
                    id,
                    mime: name.to_string(),
                }),
                _ => None,
            },
        }
    }

    /// The formats a MIME type is written as, with their bytes.
    pub(super) fn for_mime(mime: &str, bytes: &[u8]) -> Vec<(Self, Vec<u8>)> {
        match mime {
            "text/plain" => vec![(Self::Text, utf16_text(&String::from_utf8_lossy(bytes)))],
            "text/html" => vec![(
                Self::Html(register(HTML_FORMAT)),
                html_document(&String::from_utf8_lossy(bytes)),
            )],
            "image/png" => std::iter::once((Self::Png(register(PNG_FORMAT)), bytes.to_vec()))
                .chain(png_to_bitmap(bytes).map(|bitmap| (Self::Bitmap, bitmap)))
                .collect(),
            "text/uri-list" => file_list(bytes)
                .map(|files| (Self::Files, files))
                .into_iter()
                .collect(),
            mime => vec![(
                Self::Named {
                    id: register(mime),
                    mime: mime.to_string(),
                },
                bytes.to_vec(),
            )],
        }
    }

    pub(super) fn id(&self) -> u32 {
        match self {
            Self::Text => u32::from(CF_UNICODETEXT.0),
            Self::Bitmap => u32::from(CF_DIB.0),
            Self::Files => u32::from(CF_HDROP.0),
            Self::Html(id) | Self::Png(id) | Self::Named { id, .. } => *id,
        }
    }

    pub(super) fn mime(&self) -> String {
        match self {
            Self::Text => "text/plain",
            Self::Html(_) => "text/html",
            Self::Png(_) | Self::Bitmap => "image/png",
            Self::Files => "text/uri-list",
            Self::Named { mime, .. } => mime,
        }
        .to_string()
    }

    /// Bitmaps are read only when no app put a PNG on the clipboard.
    pub(super) fn preferred_over(&self, other: &Self) -> bool {
        matches!((self, other), (Self::Png(_), Self::Bitmap))
    }

    pub(super) fn to_mime(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        match self {
            Self::Text => Some(utf16_to_string(bytes).into_bytes()),
            Self::Html(_) => html_fragment(bytes),
            Self::Png(_) | Self::Named { .. } => Some(bytes.to_vec()),
            Self::Bitmap => bitmap_to_png(bytes),
            Self::Files => Some(uri_list(bytes).into_bytes()),
        }
    }
}

fn register(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(&HSTRING::from(name)) }
}

fn format_name(id: u32) -> Option<String> {
    let mut name = [0_u16; 256];
    let length = unsafe { GetClipboardFormatNameW(id, &mut name) };
    (length > 0).then(|| String::from_utf16_lossy(&name[..length as usize]))
}

fn utf16_text(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn utf16_to_string(bytes: &[u8]) -> String {
    let units = utf16_units(bytes)
        .take_while(|unit| *unit != 0)
        .collect::<Vec<_>>();
    String::from_utf16_lossy(&units)
}

fn utf16_units(bytes: &[u8]) -> impl Iterator<Item = u16> + '_ {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|unit| u16::from_le_bytes(*unit))
}

/// Wraps a fragment in the "HTML Format" header, whose offsets count UTF-8
/// bytes from the start of the data.
fn html_document(fragment: &str) -> Vec<u8> {
    const HEADER_LENGTH: usize = 105;
    let prefix = "<html><body><!--StartFragment-->";
    let suffix = "<!--EndFragment--></body></html>";
    let start_fragment = HEADER_LENGTH + prefix.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + suffix.len();
    let header = format!(
        "Version:0.9\r\nStartHTML:{HEADER_LENGTH:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
    );
    format!("{header}{prefix}{fragment}{suffix}\0").into_bytes()
}

fn html_fragment(bytes: &[u8]) -> Option<Vec<u8>> {
    let header = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let offset = |key: &str| -> Option<usize> {
        header
            .lines()
            .find_map(|line| line.strip_prefix(key))?
            .trim()
            .parse()
            .ok()
    };
    let start = offset("StartFragment:")?;
    let end = offset("EndFragment:")?.min(bytes.len());
    bytes.get(start..end).map(<[u8]>::to_vec)
}

/// A DROPFILES block of the `file:` URIs in a URI list.
fn file_list(uri_list: &[u8]) -> Option<Vec<u8>> {
    let paths = String::from_utf8_lossy(uri_list)
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| url::Url::parse(line.trim()).ok()?.to_file_path().ok())
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return None;
    }
    let mut block = Vec::new();
    for value in [20_u32, 0, 0, 0, 1] {
        block.extend_from_slice(&value.to_le_bytes());
    }
    for path in &paths {
        block.extend(
            path.to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(u16::to_le_bytes),
        );
    }
    block.extend_from_slice(&[0, 0]);
    Some(block)
}

fn uri_list(dropfiles: &[u8]) -> String {
    let read_u32 = |offset: usize| {
        dropfiles
            .get(offset..offset + 4)
            .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };
    let (Some(start), Some(wide)) = (read_u32(0), read_u32(16)) else {
        return String::new();
    };
    let names = dropfiles.get(start as usize..).unwrap_or_default();
    let paths = if wide != 0 {
        let units = utf16_units(names).collect::<Vec<_>>();
        units
            .split(|unit| *unit == 0)
            .take_while(|name| !name.is_empty())
            .map(String::from_utf16_lossy)
            .collect::<Vec<_>>()
    } else {
        names
            .split(|byte| *byte == 0)
            .take_while(|name| !name.is_empty())
            .map(|name| String::from_utf8_lossy(name).into_owned())
            .collect()
    };
    paths
        .iter()
        .filter_map(|path| url::Url::from_file_path(path).ok())
        .map(String::from)
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// A device independent bitmap is a BMP file without its file header.
fn bitmap_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    let read_u32 = |offset: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            dib.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    let header_size = read_u32(0)?;
    let bit_count = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
    let compression = read_u32(16)?;
    let colors_used = read_u32(32)?;
    let palette = if bit_count <= 8 {
        if colors_used == 0 {
            1 << bit_count
        } else {
            colors_used
        }
    } else {
        0
    } * 4;
    let masks = if compression == 3 && header_size == BITMAP_INFO_HEADER {
        12
    } else {
        0
    };
    let pixels = BITMAP_FILE_HEADER as u32 + header_size + palette + masks;
    let mut file = Vec::with_capacity(BITMAP_FILE_HEADER + dib.len());
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&((BITMAP_FILE_HEADER + dib.len()) as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&pixels.to_le_bytes());
    file.extend_from_slice(dib);
    let image = image::load_from_memory_with_format(&file, image::ImageFormat::Bmp).ok()?;
    let mut png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

/// A 32-bit bottom-up bitmap, for apps that do not read PNG.
fn png_to_bitmap(png: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let (width, height) = image.dimensions();
    let mut dib = Vec::with_capacity(BITMAP_INFO_HEADER as usize + image.as_raw().len());
    dib.extend_from_slice(&BITMAP_INFO_HEADER.to_le_bytes());
    dib.extend_from_slice(&(width as i32).to_le_bytes());
    dib.extend_from_slice(&(height as i32).to_le_bytes());
    dib.extend_from_slice(&1_u16.to_le_bytes());
    dib.extend_from_slice(&32_u16.to_le_bytes());
    dib.extend_from_slice(&[0; 24]);
    for row in image.rows().rev() {
        for pixel in row {
            let [red, green, blue, alpha] = pixel.0;
            dib.extend_from_slice(&[blue, green, red, alpha]);
        }
    }
    Some(dib)
}
