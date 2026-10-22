use std::fs::File;
use std::os::fd::AsFd;
use std::sync::Mutex;

use sabine_platform::portal::{self, Options, Response, Results};
use zbus::{Connection, Fd, OwnedVariant, Proxy, Variant};

use super::Paper;
use super::pdf::PageRanges;

const PRINT: &str = "org.freedesktop.portal.Print";
const A4: (f64, f64) = (210.0, 297.0);
const FORGOTTEN_SETTINGS: [&str; 3] = ["print-pages", "page-ranges", "n-copies"];

/// The printer and paper people last chose in this app, offered first the
/// next time it prints.
static REMEMBERED: Mutex<Option<(Results, Results)>> = Mutex::new(None);

/// One conversation with the desktop's print portal.
pub(super) struct Session {
    connection: Connection,
    print: Proxy<'static>,
}

/// What people chose in the print dialog, for the document to be laid out
/// with before it is printed.
pub(super) struct Prepared {
    pub(super) paper: Paper,
    pub(super) pages: Pages,
    pub(super) token: u32,
}

pub(super) enum Pages {
    All,
    Selection,
    Ranges(Vec<(u32, u32)>),
}

impl Prepared {
    pub(super) fn ranges(&self) -> Option<&PageRanges> {
        match &self.pages {
            Pages::Ranges(ranges) => Some(ranges),
            Pages::All | Pages::Selection => None,
        }
    }
}

impl Session {
    pub(super) async fn open(app_id: Option<&str>) -> Result<Self, String> {
        let connection = Connection::session().await.map_err(unavailable)?;
        if let Some(app_id) = app_id {
            portal::register_host_app(&connection, app_id)
                .await
                .map_err(unavailable)?;
        }
        let print = portal::proxy(&connection, PRINT)
            .await
            .map_err(unavailable)?;
        Ok(Self { connection, print })
    }

    /// Asks how to print, before the document is laid out. `None` when people
    /// cancel.
    pub(super) async fn prepare(
        &self,
        title: &str,
        has_selection: bool,
    ) -> Result<Option<Prepared>, String> {
        let (settings, page_setup) = remembered();
        let response = portal::respond(&self.connection, |token| async move {
            self.print
                .call(
                    "PreparePrint",
                    &(
                        "",
                        title,
                        settings,
                        page_setup,
                        Options::from([
                            ("handle_token", Variant::from(token)),
                            ("modal", Variant::from(true)),
                            ("has_selected_pages", Variant::from(has_selection)),
                        ]),
                    ),
                )
                .await
        })
        .await
        .map_err(unavailable)?;
        match response {
            Response::Answered(results) => prepared(results).map(Some),
            Response::Cancelled => Ok(None),
            Response::Ended => Err(unavailable("the print dialog closed")),
        }
    }

    /// Prints a PDF document, with the choices behind `token` or else after
    /// asking how. False when people cancel.
    pub(super) async fn print(
        &self,
        title: &str,
        document: &File,
        token: Option<u32>,
    ) -> Result<bool, String> {
        let response = portal::respond(&self.connection, |handle_token| async move {
            let mut options = Options::from([
                ("handle_token", Variant::from(handle_token)),
                ("modal", Variant::from(true)),
            ]);
            if let Some(token) = token {
                options.insert("token", Variant::from(token));
            }
            self.print
                .call("Print", &("", title, Fd::from(document.as_fd()), options))
                .await
        })
        .await
        .map_err(unavailable)?;
        match response {
            Response::Answered(_) => Ok(true),
            Response::Cancelled => Ok(false),
            Response::Ended => Err("The printer didn't take the document".into()),
        }
    }
}

fn prepared(mut results: Results) -> Result<Prepared, String> {
    let malformed = || "The print dialog gave an answer Sabine can't read".to_string();
    let token = results
        .get("token")
        .and_then(|token| u32::try_from(token).ok())
        .ok_or_else(malformed)?;
    let settings = dictionary(results.remove("settings"));
    let page_setup = dictionary(results.remove("page-setup"));
    let length = |key| {
        page_setup
            .get(key)
            .and_then(|value| f64::try_from(value).ok())
    };
    let (width_mm, height_mm) = length("Width").zip(length("Height")).unwrap_or(A4);
    let paper = Paper {
        width_mm,
        height_mm,
        landscape: text(&page_setup, "Orientation")
            .or_else(|| text(&settings, "orientation"))
            .is_some_and(|orientation| orientation.ends_with("landscape")),
    };
    let pages = match text(&settings, "print-pages").as_deref() {
        Some("selection") => Pages::Selection,
        Some("ranges") => text(&settings, "page-ranges")
            .and_then(|ranges| page_ranges(&ranges))
            .map_or(Pages::All, Pages::Ranges),
        _ => Pages::All,
    };
    remember(settings, page_setup);
    Ok(Prepared {
        paper,
        pages,
        token,
    })
}

/// Reads GTK's page ranges, counted from 0, as ranges counted from 1.
fn page_ranges(ranges: &str) -> Option<Vec<(u32, u32)>> {
    ranges
        .split(',')
        .map(|range| {
            let (first, last) = range.split_once('-').unwrap_or((range, range));
            let first = first.trim().parse::<u32>().ok()?;
            let last = last.trim().parse::<u32>().ok()?;
            (first <= last).then_some((first + 1, last + 1))
        })
        .collect()
}

fn dictionary(value: Option<OwnedVariant>) -> Results {
    value
        .and_then(|value| Results::try_from(value).ok())
        .unwrap_or_default()
}

fn text(values: &Results, key: &str) -> Option<String> {
    String::try_from(values.get(key)?.try_clone().ok()?).ok()
}

fn remembered() -> (Results, Results) {
    let copy = |values: &Results| -> Results {
        values
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), value.try_clone().ok()?)))
            .collect()
    };
    REMEMBERED
        .lock()
        .ok()
        .and_then(|remembered| {
            remembered
                .as_ref()
                .map(|(settings, page_setup)| (copy(settings), copy(page_setup)))
        })
        .unwrap_or_default()
}

fn remember(mut settings: Results, page_setup: Results) {
    for key in FORGOTTEN_SETTINGS {
        settings.remove(key);
    }
    if let Ok(mut remembered) = REMEMBERED.lock() {
        *remembered = Some((settings, page_setup));
    }
}

fn unavailable(error: impl std::fmt::Display) -> String {
    format!("The print dialog couldn't be opened: {error}")
}
