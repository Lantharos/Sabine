#[cfg(target_os = "linux")]
use std::fs::File;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::document::{Source, display_name, html};
use super::pdf::{PageRanges, combine};
use super::{DocumentBrowser, Paper};

/// Files a page or the app asked to print as one document.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub(in crate::osr::host) struct PrintFiles {
    paths: Vec<PathBuf>,
    title: Option<String>,
}

impl PrintFiles {
    fn title(&self) -> String {
        self.title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .or_else(|| self.paths.first().map(|path| display_name(path)))
            .unwrap_or_default()
    }

    fn sources(&self) -> Result<Vec<Source>, String> {
        if self.paths.is_empty() {
            return Err("There is nothing to print".into());
        }
        self.paths.iter().map(|path| Source::read(path)).collect()
    }

    /// Prints the files, and answers whether they went to a printer. PDF
    /// files go as they are, with the print dialog choosing their pages;
    /// images and text are laid out on the paper chosen in it.
    #[cfg(target_os = "linux")]
    pub(super) async fn print(
        self,
        app_id: Option<&str>,
        browser: &DocumentBrowser,
    ) -> Result<bool, String> {
        let title = self.title();
        let sources = self.sources()?;
        let session = super::portal::Session::open(app_id).await?;
        if let [Source::Pdf(path)] = sources.as_slice() {
            return session.print(&title, &open(path)?, None).await;
        }
        let scratch = Scratch::create()?;
        if sources.iter().all(Source::is_pdf) {
            let files = sources.iter().map(Source::path).collect::<Vec<_>>();
            let document = scratch.write(&combine(&files, None, &title)?)?;
            return session.print(&title, &document, None).await;
        }
        let Some(prepared) = session.prepare(&title, false).await? else {
            return Ok(false);
        };
        let pages = prepared.ranges();
        let combined = assemble(&sources, prepared.paper, pages, &title, browser, &scratch).await?;
        let document = scratch.write(&combined)?;
        session.print(&title, &document, Some(prepared.token)).await
    }

    /// Hands the files to the system's print dialog, as one document laid out
    /// on the default printer's paper.
    #[cfg(not(target_os = "linux"))]
    pub(super) async fn print(
        self,
        _app_id: Option<&str>,
        browser: &DocumentBrowser,
    ) -> Result<bool, String> {
        let title = self.title();
        let sources = self.sources()?;
        if let [Source::Pdf(path)] = sources.as_slice() {
            browser.show(&file_url(path)?, None).await?;
            return Ok(true);
        }
        let scratch = Scratch::create()?;
        let paper = super::paper::system_default();
        let combined = assemble(&sources, paper, None, &title, browser, &scratch).await?;
        let document = scratch.path("document.pdf");
        std::fs::write(&document, combined).map_err(preparing)?;
        browser.show(&file_url(&document)?, Some(scratch)).await?;
        Ok(true)
    }
}

/// Puts every file into one PDF: PDF files as they are, and each run of
/// images and text laid out by Chromium on `paper`.
async fn assemble(
    sources: &[Source],
    paper: Paper,
    pages: Option<&PageRanges>,
    title: &str,
    browser: &DocumentBrowser,
    scratch: &Scratch,
) -> Result<Vec<u8>, String> {
    let mut parts = Vec::new();
    for (index, group) in sources
        .chunk_by(|first, second| !first.is_pdf() && !second.is_pdf())
        .enumerate()
    {
        if let [Source::Pdf(path)] = group {
            parts.push(path.clone());
            continue;
        }
        let sheets = group.iter().filter_map(Source::sheet).collect::<Vec<_>>();
        let page = scratch.path(&format!("sheets-{index}.html"));
        std::fs::write(&page, html(&sheets)?).map_err(preparing)?;
        let output = scratch.path(&format!("sheets-{index}.pdf"));
        browser.render(&file_url(&page)?, &output, paper).await?;
        parts.push(output);
    }
    combine(
        &parts.iter().map(PathBuf::as_path).collect::<Vec<_>>(),
        pages,
        title,
    )
}

fn file_url(path: &Path) -> Result<String, String> {
    url::Url::from_file_path(path)
        .map(String::from)
        .map_err(|()| format!("{} couldn't be read", display_name(path)))
}

#[cfg(target_os = "linux")]
fn open(path: &Path) -> Result<File, String> {
    File::open(path).map_err(|_| format!("{} couldn't be read", display_name(path)))
}

fn preparing(error: std::io::Error) -> String {
    format!("The document couldn't be prepared for printing: {error}")
}

/// A private folder for one print job's files, removed with it.
pub(super) struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random).map_err(|error| error.to_string())?;
        let name = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = std::env::temp_dir().join(format!("sabine-print-{name}"));
        create_private_folder(&path).map_err(preparing)?;
        Ok(Self(path))
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    #[cfg(target_os = "linux")]
    fn write(&self, bytes: &[u8]) -> Result<File, String> {
        let path = self.path("document.pdf");
        std::fs::write(&path, bytes).map_err(preparing)?;
        open(&path)
    }
}

fn create_private_folder(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    return std::os::unix::fs::DirBuilderExt::mode(&mut std::fs::DirBuilder::new(), 0o700)
        .create(path);
    #[cfg(not(unix))]
    std::fs::create_dir(path)
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
