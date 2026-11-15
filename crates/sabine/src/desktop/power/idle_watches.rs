use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use sabine_bridge::{
    BridgeCommand, BridgeError, BridgeOutcome, BridgeResponse, DOCUMENT_CLOSED_COMMAND,
    DOCUMENT_OPENED_COMMAND, IDLE_EVENT, IDLE_UNWATCH_COMMAND, IDLE_WATCH_COMMAND,
};
use sabine_platform::IdleWatch;
use serde_json::{Value, json};

use crate::bridge::BridgeEventEmitter;

const MAX_WATCHES_PER_WINDOW: usize = 16;

/// The idle watches of the app's pages. A watch belongs to the document
/// that began it, in the browser that shows it, and ends with that document
/// as its activities do, or when the window closes.
#[derive(Clone, Default)]
pub(crate) struct IdleWatches(Arc<Mutex<Watches>>);

#[derive(Default)]
struct Watches {
    next_id: u64,
    entries: HashMap<u64, Entry>,
}

struct Entry {
    page: Page,
    /// `None` until the desktop has started the watch.
    watch: Option<IdleWatch>,
}

struct Page {
    window: u32,
    browser: String,
    document: String,
}

impl IdleWatches {
    /// Answers the idle commands of a page shown by `browser`, and ends
    /// the watches of documents that went away. Every other command is left
    /// for the app's handlers.
    pub(crate) fn dispatch(
        &self,
        command: &BridgeCommand,
        browser: &str,
        emitter: &BridgeEventEmitter,
    ) -> Option<BridgeOutcome> {
        let window = command.window?;
        let document = command.params.get("document").and_then(Value::as_str);
        match command.name.as_str() {
            IDLE_WATCH_COMMAND => Some(self.watch(command, window, browser, emitter)),
            IDLE_UNWATCH_COMMAND => {
                if let Some(id) = command.params.get("id").and_then(Value::as_u64) {
                    self.release(|watch, page| {
                        watch == id && page.window == window && page.browser == browser
                    });
                }
                Some(BridgeOutcome::Ready(Ok(BridgeResponse::json(Value::Null))))
            }
            DOCUMENT_OPENED_COMMAND | DOCUMENT_CLOSED_COMMAND => {
                let opened = command.name == DOCUMENT_OPENED_COMMAND;
                if let Some(document) = document {
                    self.release(|_, page| {
                        page.window == window
                            && page.browser == browser
                            && (page.document == document) != opened
                    });
                }
                None
            }
            _ => None,
        }
    }

    /// Ends the watches of a window that closed.
    pub(crate) fn release_window(&self, window: u32) {
        self.release(|_, page| page.window == window);
    }

    fn watch(
        &self,
        command: &BridgeCommand,
        window: u32,
        browser: &str,
        emitter: &BridgeEventEmitter,
    ) -> BridgeOutcome {
        let threshold = command
            .params
            .get("seconds")
            .and_then(Value::as_f64)
            .filter(|seconds| *seconds > 0.0)
            .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok());
        let document = command.params.get("document").and_then(Value::as_str);
        let (Some(threshold), Some(document)) = (threshold, document) else {
            return BridgeOutcome::Ready(Err(BridgeError::new(
                "watching for idleness requires positive `seconds` and a `document`",
            )));
        };
        let id = match self.add(Page {
            window,
            browser: browser.to_string(),
            document: document.to_string(),
        }) {
            Ok(id) => id,
            Err(error) => return BridgeOutcome::Ready(Err(error)),
        };
        let emitter = emitter.clone();
        let events = Arc::new(move |state: sabine_platform::IdleState| {
            emitter.emit_to(
                window,
                IDLE_EVENT,
                json!({ "watch": id, "state": state.as_str() }),
            );
        });
        let watches = self.clone();
        BridgeOutcome::Pending(Box::pin(async move {
            match IdleWatch::start(threshold, events).await {
                Ok(watch) => {
                    if let Some(entry) = watches.lock().entries.get_mut(&id) {
                        entry.watch = Some(watch);
                    }
                    Ok(BridgeResponse::json(json!({ "id": id })))
                }
                Err(error) => {
                    watches.lock().entries.remove(&id);
                    Err(BridgeError::new(error))
                }
            }
        }))
    }

    fn add(&self, page: Page) -> Result<u64, BridgeError> {
        let mut watches = self.lock();
        let window_watches = watches
            .entries
            .values()
            .filter(|entry| entry.page.window == page.window)
            .count();
        if window_watches >= MAX_WATCHES_PER_WINDOW {
            return Err(BridgeError::new(format!(
                "a window's pages can watch for idleness at most {MAX_WATCHES_PER_WINDOW} times at once"
            )));
        }
        watches.next_id += 1;
        let id = watches.next_id;
        watches.entries.insert(id, Entry { page, watch: None });
        Ok(id)
    }

    fn release(&self, ends: impl Fn(u64, &Page) -> bool) {
        self.lock()
            .entries
            .retain(|id, entry| !ends(*id, &entry.page));
    }

    fn lock(&self) -> MutexGuard<'_, Watches> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
}
