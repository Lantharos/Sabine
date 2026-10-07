use std::sync::Arc;

use serde::Deserialize;

pub(crate) const TEXT: &str = "text/plain";
const URI_LIST: &str = "text/uri-list";
const TEXT_ALIASES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    TEXT,
    "STRING",
    "TEXT",
];
const DEFAULT_TYPES: [&str; 3] = [TEXT, "text/html", URI_LIST];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Selection {
    Clipboard,
    Primary,
}

impl Selection {
    #[cfg(target_os = "linux")]
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Clipboard => 0,
            Self::Primary => 1,
        }
    }
}

/// Data on one of the desktop's selections, as MIME types and their bytes.
#[derive(Clone, Debug, Default)]
pub(crate) struct ClipboardContent {
    items: Vec<(String, Arc<[u8]>)>,
}

impl ClipboardContent {
    pub(crate) fn push(&mut self, mime: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
        self.items.push((mime.into(), bytes.into()));
    }

    pub(crate) fn items(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.items
            .iter()
            .map(|(mime, bytes)| (mime.as_str(), bytes.as_ref()))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The types to advertise: every item, plus the older names of plain text.
    #[cfg(target_os = "linux")]
    pub(crate) fn offered_types(&self) -> Vec<String> {
        let mut types = self
            .items
            .iter()
            .map(|(mime, _)| mime.clone())
            .collect::<Vec<_>>();
        if self.items.iter().any(|(mime, _)| mime == TEXT) {
            types.extend(
                TEXT_ALIASES
                    .iter()
                    .filter(|alias| **alias != TEXT)
                    .map(|alias| alias.to_string()),
            );
        }
        types
    }
    #[cfg(target_os = "linux")]
    pub(crate) fn bytes_for(&self, mime: &str) -> Option<Arc<[u8]>> {
        let wanted = if TEXT_ALIASES.contains(&mime) {
            TEXT
        } else {
            mime
        };
        self.items
            .iter()
            .find(|(item, _)| item == wanted)
            .map(|(_, bytes)| Arc::clone(bytes))
    }
}

/// Chooses which offered types to read and the type each is reported as:
/// plain text under any of its names, and by default HTML, file lists and
/// one image.
pub(crate) fn read_plan(offered: &[String], requested: Option<&[String]>) -> Vec<(String, String)> {
    let default_types = DEFAULT_TYPES.map(String::from);
    let wanted = requested.unwrap_or(&default_types);
    let mut plan = Vec::new();
    for mime in wanted {
        let source = if mime == TEXT {
            TEXT_ALIASES
                .iter()
                .find(|alias| offered.iter().any(|offer| offer == *alias))
                .map(|alias| alias.to_string())
        } else {
            offered.iter().find(|offer| *offer == mime).cloned()
        };
        if let Some(source) = source {
            plan.push((source, mime.clone()));
        }
    }
    if requested.is_none()
        && let Some(image) = offered
            .iter()
            .find(|offer| *offer == "image/png")
            .or_else(|| offered.iter().find(|offer| offer.starts_with("image/")))
    {
        plan.push((image.clone(), image.clone()));
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(types: &[&str]) -> Vec<String> {
        types.iter().map(|mime| mime.to_string()).collect()
    }

    #[test]
    fn plain_text_is_read_from_its_best_offered_name() {
        let plan = read_plan(&offered(&["STRING", "UTF8_STRING", "image/jpeg"]), None);
        assert_eq!(
            plan,
            [
                ("UTF8_STRING".into(), TEXT.into()),
                ("image/jpeg".into(), "image/jpeg".into())
            ]
        );
    }

    #[test]
    fn requested_types_limit_the_read() {
        let plan = read_plan(
            &offered(&["text/plain;charset=utf-8", "text/html", "image/png"]),
            Some(&[TEXT.to_string()]),
        );
        assert_eq!(plan, [("text/plain;charset=utf-8".into(), TEXT.into())]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn text_is_served_under_every_name() {
        let mut content = ClipboardContent::default();
        content.push(TEXT, b"hello".as_slice());
        assert!(content.offered_types().contains(&"UTF8_STRING".to_string()));
        assert_eq!(
            content.bytes_for("STRING").as_deref(),
            Some(b"hello".as_slice())
        );
    }
}
