use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};

use serde_json::{Value, json};

use crate::bridge::{BridgeCommand, BridgeError, BridgeResponse, BridgeResult};
use keep_awake::Waker;
pub use keep_awake::{KeepAwake, KeepAwakeGuard};

mod keep_awake;

const BEGIN_COMMAND: &str = "sabine.activity.begin";
const END_COMMAND: &str = "sabine.activity.end";
const LIST_COMMAND: &str = "sabine.activity.list";
/// Sent by every page as it loads, ending what the documents it replaced in
/// the same browser began, such as before a reload or a renderer crash.
pub const DOCUMENT_OPENED_COMMAND: &str = "sabine.activity.documentOpened";
/// Sent by a page as it goes away, ending what it began.
pub const DOCUMENT_CLOSED_COMMAND: &str = "sabine.activity.documentClosed";
const MAX_PAGE_ACTIVITIES_PER_WINDOW: usize = 32;

pub(super) const COMMANDS: [&str; 5] = [
    BEGIN_COMMAND,
    END_COMMAND,
    LIST_COMMAND,
    DOCUMENT_OPENED_COMMAND,
    DOCUMENT_CLOSED_COMMAND,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityOptions {
    pub name: String,
    pub keep_running: bool,
    pub keep_awake: Option<KeepAwake>,
}

impl ActivityOptions {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            keep_running: true,
            keep_awake: None,
        }
    }

    /// Keeps the machine from sleeping while the activity runs, with its
    /// name as the reason.
    pub fn keep_awake(mut self, keep_awake: KeepAwake) -> Self {
        self.keep_awake = Some(keep_awake);
        self
    }

    /// Whether the page keeps running while its window is out of sight
    /// instead of being frozen. On by default.
    pub fn keep_running(mut self, keep_running: bool) -> Self {
        self.keep_running = keep_running;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityRecord {
    pub id: String,
    pub name: String,
    pub keep_running: bool,
    pub keep_awake: Option<KeepAwake>,
    /// The window whose page began the activity, which it keeps running.
    /// The app's own activities have none and keep every window running.
    pub window: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct ActivityRegistry {
    inner: Arc<Mutex<ActivityState>>,
    waker: Waker,
}

pub struct SabineActivityLease {
    registry: ActivityRegistry,
    emitter: Option<Arc<dyn crate::ActivityEventEmitter>>,
    record: Option<ActivityRecord>,
}

#[derive(Debug, Default)]
struct ActivityState {
    next_id: u64,
    entries: BTreeMap<String, Entry>,
}

#[derive(Debug)]
struct Entry {
    record: ActivityRecord,
    page: Option<Page>,
    /// Keeps the machine awake until the activity ends.
    _awake: Option<KeepAwakeGuard>,
}

/// The document that began an activity, in the browser that shows it.
#[derive(Debug, PartialEq, Eq)]
struct Page {
    window: u32,
    browser: String,
    document: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivityHostUpdate {
    Begin(ActivityRecord),
    End(ActivityRecord),
}

impl ActivityHostUpdate {
    pub fn record(&self) -> &ActivityRecord {
        match self {
            Self::Begin(record) | Self::End(record) => record,
        }
    }
}

impl ActivityRegistry {
    /// A registry whose activities keep the machine awake through `wake`,
    /// which is handed the kind of wakefulness and the activity's name and
    /// keeps the machine awake until what it returns is dropped.
    pub fn keeping_awake(
        wake: impl Fn(KeepAwake, &str) -> KeepAwakeGuard + Send + Sync + 'static,
    ) -> Self {
        Self {
            waker: Waker::new(wake),
            ..Self::default()
        }
    }

    pub fn begin(&self, options: ActivityOptions) -> ActivityRecord {
        let mut state = self.state();
        state.insert(options, None, &self.waker)
    }

    pub fn end(&self, id: &str) -> Option<ActivityRecord> {
        self.state().entries.remove(id).map(|entry| entry.record)
    }

    pub fn list(&self) -> Vec<ActivityRecord> {
        self.state()
            .entries
            .values()
            .map(|entry| entry.record.clone())
            .collect()
    }

    /// Forgets the activities the pages of a window began, once it closed.
    pub fn release_window(&self, window: u32) {
        self.state()
            .entries
            .retain(|_, entry| entry.page.as_ref().is_none_or(|page| page.window != window));
    }

    /// Answers the activity commands of a page shown by `browser`.
    pub fn dispatch_bridge_command(
        &self,
        command: &BridgeCommand,
        browser: &str,
    ) -> Option<(BridgeResult, Vec<ActivityHostUpdate>)> {
        let window = command.window?;
        let document = || {
            command
                .params
                .get("document")
                .and_then(Value::as_str)
                .ok_or_else(|| BridgeError::new("activity commands require a `document` string"))
        };
        match command.name.as_str() {
            BEGIN_COMMAND => Some(
                match document().and_then(|document| {
                    self.begin_page(
                        options_of(&command.params),
                        Page {
                            window,
                            browser: browser.to_string(),
                            document: document.to_string(),
                        },
                    )
                }) {
                    Ok(record) => (
                        Ok(BridgeResponse::json(record_json(&record))),
                        vec![ActivityHostUpdate::Begin(record)],
                    ),
                    Err(error) => (Err(error), Vec::new()),
                },
            ),
            END_COMMAND => Some(self.dispatch_end(command, window, browser)),
            LIST_COMMAND => Some((
                Ok(BridgeResponse::json(self.snapshot_json(window))),
                Vec::new(),
            )),
            DOCUMENT_OPENED_COMMAND | DOCUMENT_CLOSED_COMMAND => Some(match document() {
                Ok(document) => {
                    let opened = command.name == DOCUMENT_OPENED_COMMAND;
                    let ended = self.release(|page| {
                        page.window == window
                            && page.browser == browser
                            && (page.document == document) != opened
                    });
                    (
                        Ok(BridgeResponse::json(Value::Null)),
                        ended.into_iter().map(ActivityHostUpdate::End).collect(),
                    )
                }
                Err(error) => (Err(error), Vec::new()),
            }),
            _ => None,
        }
    }

    pub fn lease(
        &self,
        options: ActivityOptions,
        emitter: Option<Arc<dyn crate::ActivityEventEmitter>>,
    ) -> SabineActivityLease {
        let record = self.begin(options);
        if let Some(emitter) = &emitter {
            let _ = emitter.emit_activity_update(&ActivityHostUpdate::Begin(record.clone()));
        }
        SabineActivityLease {
            registry: self.clone(),
            emitter,
            record: Some(record),
        }
    }

    fn state(&self) -> MutexGuard<'_, ActivityState> {
        self.inner.lock().expect("activity registry poisoned")
    }

    fn begin_page(
        &self,
        options: ActivityOptions,
        page: Page,
    ) -> Result<ActivityRecord, BridgeError> {
        let mut state = self.state();
        let window_activities = state
            .entries
            .values()
            .filter(|entry| entry.record.window == Some(page.window))
            .count();
        if window_activities >= MAX_PAGE_ACTIVITIES_PER_WINDOW {
            return Err(BridgeError::new(format!(
                "a window's pages can keep at most {MAX_PAGE_ACTIVITIES_PER_WINDOW} activities at once"
            )));
        }
        Ok(state.insert(options, Some(page), &self.waker))
    }

    fn release(&self, ends: impl Fn(&Page) -> bool) -> Vec<ActivityRecord> {
        let mut state = self.state();
        let ended = state
            .entries
            .iter()
            .filter(|(_, entry)| entry.page.as_ref().is_some_and(&ends))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        ended
            .iter()
            .filter_map(|id| state.entries.remove(id))
            .map(|entry| entry.record)
            .collect()
    }

    /// Pages end only the activities that pages of their own browser began.
    fn dispatch_end(
        &self,
        command: &BridgeCommand,
        window: u32,
        browser: &str,
    ) -> (BridgeResult, Vec<ActivityHostUpdate>) {
        let Some(id) = command.params.get("id").and_then(Value::as_str) else {
            return (
                Err(BridgeError::new("activity end requires an `id` string")),
                Vec::new(),
            );
        };
        let ended = {
            let mut state = self.state();
            let owned = state.entries.get(id).is_some_and(|entry| {
                entry
                    .page
                    .as_ref()
                    .is_some_and(|page| page.window == window && page.browser == browser)
            });
            owned
                .then(|| state.entries.remove(id))
                .flatten()
                .map(|entry| entry.record)
        };
        match ended {
            Some(record) => (
                Ok(BridgeResponse::json(json!({ "id": id, "ended": true }))),
                vec![ActivityHostUpdate::End(record)],
            ),
            None => (
                Ok(BridgeResponse::json(json!({ "id": id, "ended": false }))),
                Vec::new(),
            ),
        }
    }

    /// The activities that keep a window's page running: its pages' own and
    /// the app's.
    fn snapshot_json(&self, window: u32) -> Value {
        let activities = self
            .list()
            .into_iter()
            .filter(|record| record.window.is_none_or(|owner| owner == window))
            .collect::<Vec<_>>();
        json!({
            "activities": activities.iter().map(record_json).collect::<Vec<_>>(),
            "keepingRunning": activities
                .iter()
                .filter(|activity| activity.keep_running)
                .count(),
        })
    }
}

impl ActivityState {
    fn insert(
        &mut self,
        options: ActivityOptions,
        page: Option<Page>,
        waker: &Waker,
    ) -> ActivityRecord {
        self.next_id += 1;
        let record = ActivityRecord {
            id: format!("activity-{}", self.next_id),
            name: normalize_name(&options.name),
            keep_running: options.keep_running,
            keep_awake: options.keep_awake,
            window: page.as_ref().map(|page| page.window),
        };
        let awake = record
            .keep_awake
            .and_then(|keep_awake| waker.wake(keep_awake, &record.name));
        self.entries.insert(
            record.id.clone(),
            Entry {
                record: record.clone(),
                page,
                _awake: awake,
            },
        );
        record
    }
}

impl SabineActivityLease {
    pub fn id(&self) -> Option<&str> {
        self.record.as_ref().map(|record| record.id.as_str())
    }

    pub fn record(&self) -> Option<&ActivityRecord> {
        self.record.as_ref()
    }

    pub fn end(mut self) {
        self.end_inner();
    }

    fn end_inner(&mut self) {
        let Some(record) = self.record.take() else {
            return;
        };
        if let Some(record) = self.registry.end(&record.id)
            && let Some(emitter) = &self.emitter
        {
            let _ = emitter.emit_activity_update(&ActivityHostUpdate::End(record));
        }
    }
}

impl Drop for SabineActivityLease {
    fn drop(&mut self) {
        self.end_inner();
    }
}

pub fn host_update_json(update: &ActivityHostUpdate) -> Value {
    let active = matches!(update, ActivityHostUpdate::Begin(_));
    let mut value = record_json(update.record());
    value["active"] = json!(active);
    value
}

/// Activity event emitter implemented by the native CEF host. It knows how to push
/// `sabine.activity.begin` / `sabine.activity.end` events to the page.
pub trait ActivityEventEmitter: Send + Sync {
    fn emit_activity_update(&self, update: &ActivityHostUpdate) -> bool;
}

fn options_of(params: &Value) -> ActivityOptions {
    ActivityOptions {
        name: params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        keep_running: params
            .get("keepRunning")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        keep_awake: params
            .get("keepAwake")
            .and_then(Value::as_str)
            .and_then(KeepAwake::parse),
    }
}

fn normalize_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "activity".to_string();
    }
    trimmed.chars().take(128).collect()
}

fn record_json(record: &ActivityRecord) -> Value {
    json!({
        "id": record.id,
        "name": record.name,
        "keepRunning": record.keep_running,
        "keepAwake": record.keep_awake.map(KeepAwake::as_str),
    })
}

#[cfg(test)]
mod tests;
