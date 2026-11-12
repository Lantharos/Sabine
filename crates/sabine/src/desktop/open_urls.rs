use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

use sabine_platform::DeepLinkRegistration;

use crate::WindowId;

/// Links and documents waiting for the app. What a window's page takes stays
/// on record until the page leaves its document, so a page Sabine reloads,
/// such as to paint in software or after a crash, gets it again.
#[derive(Clone, Debug, Default)]
pub(crate) struct OpenUrls(Arc<Mutex<Queue>>);

#[derive(Debug, Default)]
struct Queue {
    pending: Vec<String>,
    taken: HashMap<WindowId, Vec<String>>,
}

impl OpenUrls {
    pub(crate) fn receive(&self, urls: impl IntoIterator<Item = String>) -> bool {
        let mut queue = self.queue();
        let mut added = false;
        for url in urls {
            if queue.pending.len() == 128 || url.len() > 32768 {
                let _ = sabine_runtime::record_diagnostic(
                    "desktop",
                    "URL opening queue is full or the URL exceeds 32 KiB",
                );
                continue;
            }
            queue.pending.push(url);
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
        std::mem::take(&mut self.queue().pending)
    }

    pub(crate) fn take_for_page(&self, window: WindowId) -> Vec<String> {
        let mut queue = self.queue();
        let urls = std::mem::take(&mut queue.pending);
        queue
            .taken
            .entry(window)
            .or_default()
            .extend(urls.iter().cloned());
        urls
    }

    /// Puts what the window's lost page took back in line, ahead of anything
    /// that arrived since.
    pub(crate) fn page_lost(&self, window: WindowId) {
        let mut queue = self.queue();
        if let Some(taken) = queue.taken.remove(&window) {
            queue.pending.splice(0..0, taken);
        }
    }

    /// Forgets what the window's earlier pages took, once a page replaced
    /// them or the window closed.
    pub(crate) fn release(&self, window: WindowId) {
        self.queue().taken.remove(&window);
    }

    fn queue(&self) -> MutexGuard<'_, Queue> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// The links and documents a page takes, with the files among them within
/// reach of a confined app.
pub(crate) async fn within_reach(urls: Vec<String>) -> Result<Vec<String>, String> {
    #[cfg(target_os = "linux")]
    return crate::launch::confine::documents::urls_within_reach(urls).await;
    #[cfg(not(target_os = "linux"))]
    Ok(urls)
}

/// What the app opens from its arguments: URLs with its schemes, and files
/// of its document types, by extension and, on Linux, by the type the
/// desktop gives them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Openable {
    schemes: Vec<String>,
    extensions: Vec<String>,
    #[cfg(target_os = "linux")]
    types: super::platform::DocumentTypes,
}

impl Openable {
    pub(crate) fn new(
        deep_links: &[DeepLinkRegistration],
        documents: &crate::window::DocumentTypes,
    ) -> Self {
        Self {
            schemes: deep_links
                .iter()
                .flat_map(|registration| &registration.schemes)
                .map(|scheme| scheme.to_ascii_lowercase())
                .collect(),
            extensions: documents.extensions.clone(),
            #[cfg(target_os = "linux")]
            types: super::platform::DocumentTypes::new(&documents.types),
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
        self.has_document_extension(path) || self.has_document_type(path)
    }

    fn has_document_extension(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                self.extensions
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(extension))
            })
    }

    #[cfg(target_os = "linux")]
    fn has_document_type(&self, path: &Path) -> bool {
        self.types.describe(path)
    }

    #[cfg(not(target_os = "linux"))]
    fn has_document_type(&self, _: &Path) -> bool {
        false
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
        let mut documents = crate::window::DocumentTypes::default();
        documents.extensions.push("md".to_string());
        let openable = Openable::new(
            &[DeepLinkRegistration::new("com.sabine.notes", ["notes"])],
            &documents,
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

    #[test]
    fn a_lost_page_gets_what_it_took_again() {
        let urls = OpenUrls::default();
        urls.receive(["notes://first".to_string()]);
        assert_eq!(urls.take_for_page(7), ["notes://first"]);
        urls.receive(["notes://second".to_string()]);
        urls.page_lost(7);
        assert_eq!(urls.take_for_page(7), ["notes://first", "notes://second"]);
        urls.release(7);
        urls.page_lost(7);
        assert!(urls.take_for_page(7).is_empty());
    }
}
