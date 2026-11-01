use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use sabine_platform::DeepLinkRegistration;

#[derive(Clone, Debug, Default)]
pub(crate) struct OpenUrls(Arc<Mutex<Vec<String>>>);

impl OpenUrls {
    pub(crate) fn receive(&self, urls: impl IntoIterator<Item = String>) -> bool {
        let mut pending = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let mut added = false;
        for url in urls {
            if pending.len() == 128 || url.len() > 32768 {
                let _ = sabine_runtime::record_diagnostic(
                    "desktop",
                    "URL opening queue is full or the URL exceeds 32 KiB",
                );
                continue;
            }
            pending.push(url);
            added = true;
        }
        added
    }

    /// Queues the arguments that name something the app opens. `arguments`
    /// excludes the program itself.
    pub(crate) fn receive_arguments(
        &self,
        arguments: &[String],
        directory: Option<&Path>,
        openable: &Openable,
    ) -> bool {
        self.receive(
            arguments
                .iter()
                .filter(|argument| !argument.starts_with('-'))
                .filter_map(|argument| openable.url(argument, directory)),
        )
    }

    pub(crate) fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|error| error.into_inner()))
    }
}

/// What the app opens from its arguments: URLs with its schemes, and files
/// with the extensions of its document types.
#[derive(Clone, Debug, Default)]
pub(crate) struct Openable {
    schemes: Vec<String>,
    extensions: Vec<String>,
}

impl Openable {
    pub(crate) fn new(deep_links: &[DeepLinkRegistration], extensions: &[String]) -> Self {
        Self {
            schemes: deep_links
                .iter()
                .flat_map(|registration| &registration.schemes)
                .map(|scheme| scheme.to_ascii_lowercase())
                .collect(),
            extensions: extensions.to_vec(),
        }
    }

    fn url(&self, argument: &str, directory: Option<&Path>) -> Option<String> {
        if let Ok(url) = url::Url::parse(argument) {
            if url.scheme() == "file" {
                return url
                    .to_file_path()
                    .is_ok_and(|path| self.is_document(&path))
                    .then(|| url.into());
            }
            if self.schemes.iter().any(|scheme| scheme == url.scheme()) {
                return Some(url.into());
            }
        }
        let path = directory?.join(argument);
        (path.is_file() && self.is_document(&path))
            .then(|| url::Url::from_file_path(path).ok().map(Into::into))
            .flatten()
    }

    fn is_document(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                self.extensions
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(extension))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_documents_and_app_urls_open() {
        let directory = std::env::temp_dir().join(format!("sabine-open-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("note.md"), "").unwrap();
        std::fs::write(directory.join("config.toml"), "").unwrap();
        let openable = Openable::new(
            &[DeepLinkRegistration::new("com.sabine.notes", ["notes"])],
            &["md".to_string()],
        );
        let urls = OpenUrls::default();
        urls.receive_arguments(
            &[
                "--profile".into(),
                "config.toml".into(),
                "note.md".into(),
                "notes://open".into(),
                "https://example.com".into(),
            ],
            Some(&directory),
            &openable,
        );
        let opened = urls.take();
        std::fs::remove_dir_all(&directory).unwrap();
        assert_eq!(opened.len(), 2);
        assert!(opened[0].ends_with("/note.md"));
        assert_eq!(opened[1], "notes://open");
    }
}
