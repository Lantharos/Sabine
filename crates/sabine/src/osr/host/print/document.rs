use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use image::ImageFormat;

const SNIFF_BYTES: u64 = 8192;
const PDF_HEADER_WITHIN: usize = 1024;
const BROWSER_IMAGES: [ImageFormat; 7] = [
    ImageFormat::Png,
    ImageFormat::Jpeg,
    ImageFormat::Gif,
    ImageFormat::WebP,
    ImageFormat::Bmp,
    ImageFormat::Ico,
    ImageFormat::Avif,
];

const STYLE: &str = "@page{margin:12mm}\
html,body{margin:0;background:#fff;color:#000}\
section+section{break-before:page}\
.image{height:100vh;display:flex;align-items:center;justify-content:center}\
.image img{max-width:100%;max-height:100%;object-fit:contain}\
pre{margin:0;font:10pt/1.45 monospace;white-space:pre-wrap;overflow-wrap:anywhere;tab-size:4}";

/// A file to print, by what its contents are.
pub(super) enum Source {
    Pdf(PathBuf),
    Sheet(Sheet),
}

/// A file Chromium lays out on paper.
pub(super) enum Sheet {
    Image(PathBuf),
    Text(PathBuf),
}

impl Source {
    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let mut head = Vec::new();
        File::open(path)
            .and_then(|file| file.take(SNIFF_BYTES).read_to_end(&mut head))
            .map_err(|_| unreadable(path))?;
        let path = path.to_path_buf();
        if head
            .windows(5)
            .take(PDF_HEADER_WITHIN)
            .any(|window| window == b"%PDF-")
        {
            Ok(Self::Pdf(path))
        } else if image::guess_format(&head).is_ok_and(|format| BROWSER_IMAGES.contains(&format))
            || is_svg(&path, &head)
        {
            Ok(Self::Sheet(Sheet::Image(path)))
        } else if !head.contains(&0) {
            Ok(Self::Sheet(Sheet::Text(path)))
        } else {
            Err(format!("{} can't be printed", display_name(&path)))
        }
    }

    pub(super) fn is_pdf(&self) -> bool {
        matches!(self, Self::Pdf(_))
    }

    #[cfg(target_os = "linux")]
    pub(super) fn path(&self) -> &Path {
        match self {
            Self::Pdf(path) | Self::Sheet(Sheet::Image(path) | Sheet::Text(path)) => path,
        }
    }

    pub(super) fn sheet(&self) -> Option<&Sheet> {
        match self {
            Self::Sheet(sheet) => Some(sheet),
            Self::Pdf(_) => None,
        }
    }
}

impl Sheet {
    fn section(&self) -> Result<String, String> {
        match self {
            Self::Image(path) => {
                let url = url::Url::from_file_path(path).map_err(|()| unreadable(path))?;
                Ok(format!(
                    "<section class=image><img src=\"{}\"></section>",
                    escape(url.as_str())
                ))
            }
            Self::Text(path) => {
                let text = std::fs::read(path).map_err(|_| unreadable(path))?;
                Ok(format!(
                    "<section><pre>{}</pre></section>",
                    escape(&String::from_utf8_lossy(&text))
                ))
            }
        }
    }
}

/// One HTML page holding each image and text file, every one starting on a
/// new sheet.
pub(super) fn html(sheets: &[&Sheet]) -> Result<String, String> {
    let mut page = format!("<!doctype html><meta charset=utf-8><style>{STYLE}</style>");
    for sheet in sheets {
        page.push_str(&sheet.section()?);
    }
    Ok(page)
}

fn unreadable(path: &Path) -> String {
    format!("{} couldn't be read", display_name(path))
}

pub(super) fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

fn is_svg(path: &Path, head: &[u8]) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        && String::from_utf8_lossy(head).contains("<svg")
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(character),
        }
    }
    escaped
}
