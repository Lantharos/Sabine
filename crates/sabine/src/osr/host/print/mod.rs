mod document;
mod files;
#[cfg(target_os = "linux")]
mod page;
#[cfg(not(target_os = "linux"))]
mod paper;
mod pdf;
#[cfg(target_os = "linux")]
mod portal;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use super::events::bridge_response_line;
use super::native::OsrNativeHost;
use super::page::BridgeRequest;
use crate::osr::control::ControlWriter;
use crate::osr::protocol::PrintDocumentOutcome;
pub(in crate::osr::host) use files::PrintFiles;

/// What this window prints: the documents its browser loads out of sight,
/// and on Linux the print dialogs its pages are waiting for.
#[derive(Clone, Default)]
pub(in crate::osr::host) struct Printing {
    documents: Arc<Mutex<Documents>>,
    #[cfg(target_os = "linux")]
    page_tokens: Arc<Mutex<HashMap<i32, u32>>>,
}

#[derive(Default)]
struct Documents {
    next_id: u32,
    waiting: HashMap<u32, oneshot::Sender<bool>>,
    #[cfg(not(target_os = "linux"))]
    shown: Option<files::Scratch>,
}

/// A page size in millimeters, as the paper is fed: portrait.
#[derive(Clone, Copy)]
pub(super) struct Paper {
    width_mm: f64,
    height_mm: f64,
    landscape: bool,
}

/// The browser behind one print job, which renders and prints documents.
#[derive(Clone)]
struct DocumentBrowser {
    writer: Arc<ControlWriter>,
    printing: Printing,
}

impl DocumentBrowser {
    /// Renders the page at `url` into a PDF file at `output`.
    async fn render(&self, url: &str, output: &Path, paper: Paper) -> Result<(), String> {
        let target = format!(
            "{}\t{}\t{}\t{}",
            output.display(),
            paper.width_mm,
            paper.height_mm,
            u8::from(paper.landscape)
        );
        self.open(url, &target)
            .await
            .then_some(())
            .ok_or_else(|| "The document couldn't be prepared for printing".to_string())
    }

    /// Hands the document at `url` to the system's print dialog.
    #[cfg(not(target_os = "linux"))]
    async fn show(&self, url: &str, scratch: Option<files::Scratch>) -> Result<(), String> {
        let shown = self.open(url, "\t0\t0\t0").await;
        if let Ok(mut documents) = self.printing.documents.lock() {
            documents.shown = scratch;
        }
        shown
            .then_some(())
            .ok_or_else(|| "The print dialog couldn't be opened".to_string())
    }

    async fn open(&self, url: &str, target: &str) -> bool {
        let Some((id, outcome)) = self.printing.await_document() else {
            return false;
        };
        self.writer
            .send(format!("print_document\t{id}\t{url}\t{target}\n"))
            .is_ok()
            && outcome.await.unwrap_or(false)
    }
}

impl Printing {
    fn await_document(&self) -> Option<(u32, oneshot::Receiver<bool>)> {
        let mut documents = self.documents.lock().ok()?;
        let (sender, receiver) = oneshot::channel();
        documents.next_id += 1;
        let id = documents.next_id;
        documents.waiting.insert(id, sender);
        Some((id, receiver))
    }

    fn finish_document(&self, outcome: PrintDocumentOutcome) {
        let waiting = self
            .documents
            .lock()
            .ok()
            .and_then(|mut documents| documents.waiting.remove(&outcome.id));
        if let Some(waiting) = waiting {
            let _ = waiting.send(outcome.ok);
        }
    }

    fn abandon_documents(&self) {
        if let Ok(mut documents) = self.documents.lock() {
            documents.waiting.clear();
        }
    }
}

impl OsrNativeHost {
    fn document_browser(&self) -> Option<DocumentBrowser> {
        Some(DocumentBrowser {
            writer: self.control_writer.clone()?,
            printing: self.printing.clone(),
        })
    }

    pub(super) fn answer_print(&mut self, request: &BridgeRequest) {
        let job = serde_json::from_str::<PrintFiles>(request.payload)
            .map_err(|error| error.to_string())
            .and_then(|files| {
                self.document_browser()
                    .map(|browser| (files, browser))
                    .ok_or_else(|| "The window isn't ready to print".to_string())
            });
        let (files, browser) = match job {
            Ok(job) => job,
            Err(error) => {
                self.send_bridge_response(request.browser_id, request.request_id, Err(error));
                return;
            }
        };
        let app_id = self.config.app_id.clone();
        let browser_id = request.browser_id.to_string();
        let request_id = request.request_id.to_string();
        sabine_platform::async_runtime().spawn(async move {
            let writer = browser.writer.clone();
            let result = files
                .print(app_id.as_deref(), &browser)
                .await
                .map(serde_json::Value::Bool);
            let _ = writer.send(bridge_response_line(&browser_id, &request_id, result));
        });
    }

    pub(in crate::osr::host) fn print_files(&self, files: PrintFiles) {
        let Some(browser) = self.document_browser() else {
            sabine_runtime::report_error("print", "the window isn't ready to print");
            return;
        };
        let app_id = self.config.app_id.clone();
        sabine_platform::async_runtime().spawn(async move {
            if let Err(error) = files.print(app_id.as_deref(), &browser).await {
                sabine_runtime::report_error("print", error);
            }
        });
    }

    pub(in crate::osr::host) fn finish_print_document(&self, outcome: PrintDocumentOutcome) {
        self.printing.finish_document(outcome);
    }

    pub(in crate::osr::host) fn abandon_printing(&self) {
        self.printing.abandon_documents();
    }
}
