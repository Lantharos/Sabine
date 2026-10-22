use std::fs::File;

use super::portal::{Pages, Prepared, Session};
use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::{PrintDialogRequest, PrintJobRequest};

impl OsrNativeHost {
    /// Asks the desktop how to print a page, for Chromium to lay it out.
    pub(in crate::osr::host) fn show_page_print_dialog(&self, request: PrintDialogRequest) {
        let Some(writer) = self.control_writer.clone() else {
            return;
        };
        let app_id = self.config.app_id.clone();
        let tokens = self.printing.page_tokens.clone();
        sabine_platform::async_runtime().spawn(async move {
            let prepared = match Session::open(app_id.as_deref()).await {
                Ok(session) => session.prepare("", request.has_selection).await,
                Err(error) => Err(error),
            };
            let answer = match prepared {
                Ok(Some(prepared)) => {
                    if let Ok(mut tokens) = tokens.lock() {
                        tokens.insert(request.browser, prepared.token);
                    }
                    chromium_settings(&prepared)
                }
                Ok(None) => String::new(),
                Err(error) => {
                    sabine_runtime::report_error("print", error);
                    String::new()
                }
            };
            let _ = writer.send(format!("print_dialog\t{}{answer}\n", request.id));
        });
    }

    /// Prints the PDF Chromium made of a page, with the choices made for it.
    pub(in crate::osr::host) fn print_page(&self, job: PrintJobRequest) {
        let Some(writer) = self.control_writer.clone() else {
            return;
        };
        let document = File::open(&job.path);
        let token = self
            .printing
            .page_tokens
            .lock()
            .ok()
            .and_then(|mut tokens| tokens.remove(&job.browser));
        let app_id = self.config.app_id.clone();
        sabine_platform::async_runtime().spawn(async move {
            let printed = match document {
                Ok(document) => match Session::open(app_id.as_deref()).await {
                    Ok(session) => session.print(&job.title, &document, token).await,
                    Err(error) => Err(error),
                },
                Err(error) => Err(format!("The printed page couldn't be read: {error}")),
            };
            if let Err(error) = printed {
                sabine_runtime::report_error("print", error);
            }
            let _ = writer.send(format!("print_job\t{}\n", job.id));
        });
    }
}

fn chromium_settings(prepared: &Prepared) -> String {
    let ranges = match &prepared.pages {
        Pages::Ranges(ranges) => ranges
            .iter()
            .map(|(first, last)| format!("{}-{}", first - 1, last - 1))
            .collect::<Vec<_>>()
            .join(","),
        Pages::All | Pages::Selection => String::new(),
    };
    format!(
        "\t{ranges}\t{}\t{}\t{}\t{}",
        prepared.paper.width_mm,
        prepared.paper.height_mm,
        u8::from(prepared.paper.landscape),
        u8::from(matches!(prepared.pages, Pages::Selection)),
    )
}
