use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString, NSURL};

use super::ClipboardContent;
use super::content::read_plan;
use super::worker::Pasteboard;

const FILE_URL: &str = "public.file-url";
const URI_LIST: &str = "text/uri-list";

/// MIME types and the pasteboard types macOS apps use for them. Any other
/// MIME type travels under its own name.
const TYPES: [(&str, &str); 6] = [
    ("text/plain", "public.utf8-plain-text"),
    ("text/html", "public.html"),
    ("text/rtf", "public.rtf"),
    ("image/png", "public.png"),
    ("image/jpeg", "public.jpeg"),
    ("image/tiff", "public.tiff"),
];

pub(super) struct GeneralPasteboard;

impl Pasteboard for GeneralPasteboard {
    fn read(types: Option<&[String]>) -> Result<ClipboardContent, String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        let offered = pasteboard
            .types()
            .map(|types| {
                types
                    .iter()
                    .map(|uti| mime_for(&uti.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut content = ClipboardContent::default();
        for (source, mime) in read_plan(&offered, types) {
            let bytes = if source == URI_LIST {
                file_urls(&pasteboard).into_bytes()
            } else {
                let Some(data) =
                    pasteboard.dataForType(&NSString::from_str(pasteboard_type(&source)))
                else {
                    continue;
                };
                data.to_vec()
            };
            content.push(mime, bytes);
        }
        Ok(content)
    }

    fn write(content: &ClipboardContent) -> Result<(), String> {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        if let Some((_, uris)) = content.items().find(|(mime, _)| *mime == URI_LIST) {
            let urls = String::from_utf8_lossy(uris)
                .lines()
                .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
                .filter_map(|line| NSURL::URLWithString(&NSString::from_str(line.trim())))
                .map(ProtocolObject::<dyn NSPasteboardWriting>::from_retained)
                .collect::<Vec<Retained<_>>>();
            if !urls.is_empty() && !pasteboard.writeObjects(&NSArray::from_retained_slice(&urls)) {
                return Err("Could not put the files on the pasteboard".to_string());
            }
        }
        for (mime, bytes) in content.items().filter(|(mime, _)| *mime != URI_LIST) {
            if !pasteboard.setData_forType(
                Some(&NSData::with_bytes(bytes)),
                &NSString::from_str(pasteboard_type(mime)),
            ) {
                return Err(format!("Could not put {mime} on the pasteboard"));
            }
        }
        Ok(())
    }
}

fn mime_for(pasteboard_type: &str) -> String {
    if pasteboard_type == FILE_URL {
        return URI_LIST.to_string();
    }
    TYPES
        .iter()
        .find(|(_, uti)| *uti == pasteboard_type)
        .map_or(pasteboard_type, |(mime, _)| mime)
        .to_string()
}

fn pasteboard_type(mime: &str) -> &str {
    TYPES
        .iter()
        .find(|(known, _)| *known == mime)
        .map_or(mime, |(_, uti)| uti)
}

/// Every file on the pasteboard, one per item, as a URI list.
fn file_urls(pasteboard: &NSPasteboard) -> String {
    let file_url = NSString::from_str(FILE_URL);
    pasteboard
        .pasteboardItems()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.stringForType(&file_url))
                .map(|url| url.to_string())
                .collect::<Vec<_>>()
                .join("\r\n")
        })
        .unwrap_or_default()
}
