use std::path::Path;
use std::sync::{Arc, OnceLock};

use mime::Mime;
use xdg_mime::SharedMimeInfo;

/// The document types an app declares, matched against files the way the
/// desktop's shared MIME database names them, so a file the desktop opens
/// with the app counts as one of its documents whatever its extension.
#[derive(Clone, Default)]
pub(crate) struct DocumentTypes {
    types: Vec<Mime>,
    database: Arc<OnceLock<SharedMimeInfo>>,
}

impl std::fmt::Debug for DocumentTypes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_list().entries(&self.types).finish()
    }
}

impl DocumentTypes {
    pub(crate) fn new(types: &[String]) -> Self {
        Self {
            types: types.iter().filter_map(|name| name.parse().ok()).collect(),
            database: Arc::default(),
        }
    }

    pub(crate) fn describe(&self, path: &Path) -> bool {
        if self.types.is_empty() {
            return false;
        }
        let database = self.database.get_or_init(SharedMimeInfo::new);
        let named = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| database.get_mime_types_from_file_name(name))
            .unwrap_or_default();
        let guessed = database
            .guess_mime_type()
            .path(path)
            .guess()
            .mime_type()
            .clone();
        named
            .iter()
            .filter(|named| **named != mime::APPLICATION_OCTET_STREAM)
            .chain([&guessed])
            .any(|found| {
                self.types
                    .iter()
                    .any(|declared| database.mime_type_subclass(found, declared))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_files_are_text_documents_whatever_their_names() {
        let folder = std::env::temp_dir().join(format!("sabine-types-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for (name, contents) in [
            ("note.txt", "hello\n".as_bytes()),
            ("Makefile", b"all:\n\ttrue\n"),
            ("README", b"just words\n"),
            ("blob", b"\x00\x01\x02\x03"),
        ] {
            std::fs::write(folder.join(name), contents).unwrap();
        }
        let text = DocumentTypes::new(&["text/plain".into()]);
        let markdown = DocumentTypes::new(&["text/markdown".into()]);
        let opened = |types: &DocumentTypes, name: &str| types.describe(&folder.join(name));
        assert!(opened(&text, "note.txt"));
        assert!(opened(&text, "Makefile"));
        assert!(opened(&text, "README"));
        assert!(!opened(&text, "blob"));
        assert!(!opened(&markdown, "note.txt"));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
