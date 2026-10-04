use std::collections::BTreeMap;

use serde::Deserialize;

const SCHEME_PREFIX: &str = "x-scheme-handler/";

/// The URL schemes and documents an app opens, from `[app]` in `Sabine.toml`.
/// `extensions` maps document MIME types to the file extensions they use.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub(crate) struct Associations {
    #[serde(default)]
    pub mime_types: Vec<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Vec<String>>,
}

/// The Windows ProgID that opens an app's documents.
pub(crate) fn prog_id(app_id: &str) -> String {
    format!("{app_id}.document")
}

/// A document type the app opens.
pub(crate) struct Document<'a> {
    pub mime_type: &'a str,
    pub extensions: &'a [String],
}

impl Associations {
    pub(crate) fn validate(&self) -> Result<(), String> {
        for mime in &self.mime_types {
            let valid = mime.split_once('/').is_some_and(|(kind, subtype)| {
                !kind.is_empty() && !subtype.is_empty() && !subtype.contains('/')
            }) && mime
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/-._+".contains(&byte));
            if !valid {
                return Err(format!("invalid app MIME type: {mime}"));
            }
            if let Some(scheme) = mime.strip_prefix(SCHEME_PREFIX)
                && (!scheme
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphabetic)
                    || !scheme.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"+.-".contains(&byte)
                    }))
            {
                return Err(format!(
                    "invalid app URL scheme: {scheme}; use a lowercase URI scheme"
                ));
            }
        }
        for (mime, extensions) in &self.extensions {
            if mime.starts_with(SCHEME_PREFIX) || !self.mime_types.contains(mime) {
                return Err(format!(
                    "app extensions list {mime}, which is not a document type in app.mime_types"
                ));
            }
            if let Some(extension) = extensions.iter().find(|extension| {
                !extension
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                    || !extension.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"+-_".contains(&byte)
                    })
            }) {
                return Err(format!(
                    "invalid file extension {extension:?} for {mime}; use lowercase letters and digits without a leading dot"
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn schemes(&self) -> impl Iterator<Item = &str> {
        self.mime_types
            .iter()
            .filter_map(|mime| mime.strip_prefix(SCHEME_PREFIX))
    }

    pub(crate) fn documents(&self) -> impl Iterator<Item = Document<'_>> {
        self.mime_types
            .iter()
            .filter(|mime| !mime.starts_with(SCHEME_PREFIX))
            .map(|mime| Document {
                mime_type: mime,
                extensions: self.extensions.get(mime).map_or(&[], Vec::as_slice),
            })
    }

    pub(crate) fn extensions(&self) -> impl Iterator<Item = &str> {
        self.documents()
            .flat_map(|document| document.extensions)
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn associations(extensions: &[(&str, &[&str])]) -> Associations {
        Associations {
            mime_types: vec![
                "text/markdown".to_string(),
                "x-scheme-handler/notes".to_string(),
            ],
            extensions: extensions
                .iter()
                .map(|(mime, extensions)| {
                    (
                        mime.to_string(),
                        extensions.iter().map(ToString::to_string).collect(),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn extensions_belong_to_declared_document_types() {
        assert!(
            associations(&[("text/markdown", &["md", "markdown"])])
                .validate()
                .is_ok()
        );
        assert!(
            associations(&[("text/plain", &["txt"])])
                .validate()
                .is_err()
        );
        assert!(
            associations(&[("x-scheme-handler/notes", &["notes"])])
                .validate()
                .is_err()
        );
        assert!(
            associations(&[("text/markdown", &[".md"])])
                .validate()
                .is_err()
        );
        assert!(
            associations(&[("text/markdown", &["MD"])])
                .validate()
                .is_err()
        );
    }
}
