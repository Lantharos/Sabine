use super::{macos::xml, types::Document};

/// Uniform Type Identifiers macOS itself declares for common MIME types. Apps
/// list these directly; declaring them again would hide the system type.
const SYSTEM_TYPES: &[(&str, &str)] = &[
    ("application/epub+zip", "org.idpf.epub-container"),
    ("application/gzip", "org.gnu.gnu-zip-archive"),
    ("application/json", "public.json"),
    ("application/msword", "com.microsoft.word.doc"),
    ("application/pdf", "com.adobe.pdf"),
    ("application/rtf", "public.rtf"),
    ("application/vnd.ms-excel", "com.microsoft.excel.xls"),
    (
        "application/vnd.ms-powerpoint",
        "com.microsoft.powerpoint.ppt",
    ),
    (
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "org.openxmlformats.presentationml.presentation",
    ),
    (
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "org.openxmlformats.spreadsheetml.sheet",
    ),
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "org.openxmlformats.wordprocessingml.document",
    ),
    ("application/x-sh", "public.shell-script"),
    ("application/x-tar", "public.tar-archive"),
    ("application/xml", "public.xml"),
    ("application/zip", "public.zip-archive"),
    ("audio/aac", "public.aac-audio"),
    ("audio/aiff", "public.aiff-audio"),
    ("audio/mp4", "public.mpeg-4-audio"),
    ("audio/mpeg", "public.mp3"),
    ("audio/wav", "com.microsoft.waveform-audio"),
    ("font/otf", "public.opentype-font"),
    ("font/ttf", "public.truetype-ttf-font"),
    ("image/bmp", "com.microsoft.bmp"),
    ("image/gif", "com.compuserve.gif"),
    ("image/heic", "public.heic"),
    ("image/jpeg", "public.jpeg"),
    ("image/png", "public.png"),
    ("image/svg+xml", "public.svg-image"),
    ("image/tiff", "public.tiff"),
    ("image/webp", "org.webmproject.webp"),
    ("image/x-icon", "com.microsoft.ico"),
    ("text/calendar", "com.apple.ical.ics"),
    ("text/csv", "public.comma-separated-values-text"),
    ("text/html", "public.html"),
    ("text/javascript", "com.netscape.javascript-source"),
    ("text/plain", "public.plain-text"),
    ("text/rtf", "public.rtf"),
    (
        "text/tab-separated-values",
        "public.tab-separated-values-text",
    ),
    ("text/vcard", "public.vcard"),
    ("text/x-c", "public.c-source"),
    ("text/x-c++", "public.c-plus-plus-source"),
    ("text/x-python", "public.python-script"),
    ("text/x-swift", "public.swift-source"),
    ("text/xml", "public.xml"),
    ("text/yaml", "public.yaml"),
    ("video/mp4", "public.mpeg-4"),
    ("video/mpeg", "public.mpeg"),
    ("video/quicktime", "com.apple.quicktime-movie"),
];

/// Identifiers other apps already use for types macOS may not declare itself.
/// Apps import them so every app agrees on one identifier.
const SHARED_TYPES: &[(&str, &str)] = &[("text/markdown", "net.daringfireball.markdown")];

/// `CFBundleDocumentTypes` entries naming each document's type identifier,
/// and `UTImportedTypeDeclarations` for the types macOS does not know.
pub(crate) fn document_types<'a>(
    app_id: &str,
    documents: impl Iterator<Item = Document<'a>>,
) -> String {
    let mut handlers = String::new();
    let mut declarations = String::new();
    for document in documents {
        let identifier = match known_type(SYSTEM_TYPES, document.mime_type) {
            Some(identifier) => identifier.to_string(),
            None => {
                let identifier = known_type(SHARED_TYPES, document.mime_type)
                    .map(str::to_string)
                    .unwrap_or_else(|| imported_identifier(app_id, document.mime_type));
                declarations.push_str(&declaration(&identifier, &document));
                identifier
            }
        };
        handlers.push_str(&format!(
            "<dict><key>CFBundleTypeName</key><string>{}</string><key>CFBundleTypeRole</key><string>Viewer</string><key>LSHandlerRank</key><string>Alternate</string><key>LSItemContentTypes</key><array><string>{}</string></array></dict>",
            xml(document.mime_type),
            xml(&identifier)
        ));
    }
    let mut plist = String::new();
    if !handlers.is_empty() {
        plist.push_str(&format!(
            "<key>CFBundleDocumentTypes</key><array>{handlers}</array>\n"
        ));
    }
    if !declarations.is_empty() {
        plist.push_str(&format!(
            "<key>UTImportedTypeDeclarations</key><array>{declarations}</array>\n"
        ));
    }
    plist
}

fn known_type(types: &[(&str, &'static str)], mime_type: &str) -> Option<&'static str> {
    types
        .iter()
        .find(|(mime, _)| *mime == mime_type)
        .map(|(_, identifier)| *identifier)
}

fn imported_identifier(app_id: &str, mime_type: &str) -> String {
    let suffix = mime_type
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>();
    format!("{app_id}.{suffix}")
}

fn declaration(identifier: &str, document: &Document) -> String {
    let conforms_to = match document.mime_type.split_once('/').map(|(kind, _)| kind) {
        Some("text") => "public.text",
        Some("image") => "public.image",
        Some("audio") => "public.audio",
        Some("video") => "public.movie",
        _ => "public.data",
    };
    let extensions = document
        .extensions
        .iter()
        .map(|extension| format!("<string>{}</string>", xml(extension)))
        .collect::<String>();
    format!(
        "<dict><key>UTTypeIdentifier</key><string>{}</string><key>UTTypeDescription</key><string>{}</string><key>UTTypeConformsTo</key><array><string>{conforms_to}</string></array><key>UTTypeTagSpecification</key><dict><key>public.mime-type</key><array><string>{}</string></array><key>public.filename-extension</key><array>{extensions}</array></dict></dict>",
        xml(identifier),
        xml(document.mime_type),
        xml(document.mime_type)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::types::Associations;

    #[test]
    fn declares_only_types_macos_does_not_know() {
        let associations = Associations {
            mime_types: vec![
                "text/plain".to_string(),
                "text/markdown".to_string(),
                "application/vnd.example.notes+json".to_string(),
            ],
            extensions: [(
                "application/vnd.example.notes+json".to_string(),
                vec!["notes".to_string()],
            )]
            .into(),
        };
        let plist = document_types("com.example.notes", associations.documents());
        let declarations = &plist[plist.find("UTImportedTypeDeclarations").unwrap()..];
        assert!(!declarations.contains("public.plain-text"));
        assert!(
            declarations.contains(
                "<key>UTTypeIdentifier</key><string>net.daringfireball.markdown</string>"
            )
        );
        assert!(declarations.contains(
            "<key>UTTypeIdentifier</key><string>com.example.notes.application-vnd-example-notes-json</string>"
        ));
        assert!(
            declarations.contains(
                "<key>public.filename-extension</key><array><string>notes</string></array>"
            )
        );
    }
}
