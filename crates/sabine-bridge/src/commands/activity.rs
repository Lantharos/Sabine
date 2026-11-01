use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};

use serde_json::{Value, json};

use crate::bridge::{BridgeCommand, BridgeError, BridgeResponse, BridgeResult};

const BEGIN_COMMAND: &str = "sabine.activity.begin";
const END_COMMAND: &str = "sabine.activity.end";
const LIST_COMMAND: &str = "sabine.activity.list";
/// Sent by every page as it loads, ending what the documents it replaced in
/// the same browser began, such as before a reload or a renderer crash.
const DOCUMENT_OPENED_COMMAND: &str = "sabine.activity.documentOpened";
/// Sent by a page as it goes away, ending what it began.
const DOCUMENT_CLOSED_COMMAND: &str = "sabine.activity.documentClosed";

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
}

impl ActivityOptions {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            keep_running: true,
        }
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
    /// The window whose page began the activity, which it keeps running.
    /// The app's own activities have none and keep every window running.
    pub window: Option<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct ActivityRegistry {
    inner: Arc<Mutex<ActivityState>>,
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
    pub fn begin(&self, options: ActivityOptions) -> ActivityRecord {
        self.insert(options, None)
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
            BEGIN_COMMAND => Some(match document() {
                Ok(document) => {
                    let page = Page {
                        window,
                        browser: browser.to_string(),
                        document: document.to_string(),
                    };
                    let record = self.insert(options_of(&command.params), Some(page));
                    (
                        Ok(BridgeResponse::json(record_json(&record))),
                        vec![ActivityHostUpdate::Begin(record)],
                    )
                }
                Err(error) => (Err(error), Vec::new()),
            }),
            END_COMMAND => Some(self.dispatch_end(command, window)),
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

    fn insert(&self, options: ActivityOptions, page: Option<Page>) -> ActivityRecord {
        let mut state = self.state();
        state.next_id += 1;
        let record = ActivityRecord {
            id: format!("activity-{}", state.next_id),
            name: normalize_name(&options.name),
            keep_running: options.keep_running,
            window: page.as_ref().map(|page| page.window),
        };
        state.entries.insert(
            record.id.clone(),
            Entry {
                record: record.clone(),
                page,
            },
        );
        record
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

    /// Pages end only the activities their own window's pages began.
    fn dispatch_end(
        &self,
        command: &BridgeCommand,
        window: u32,
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
                    .is_some_and(|page| page.window == window)
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingEmitter(Arc<AtomicUsize>);

    impl ActivityEventEmitter for CountingEmitter {
        fn emit_activity_update(&self, _update: &ActivityHostUpdate) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst);
            true
        }
    }

    fn command(name: &str, window: u32, params: Value) -> BridgeCommand {
        BridgeCommand {
            name: name.to_string(),
            params,
            origin: None,
            body: None,
            window: Some(window),
        }
    }

    fn begin(registry: &ActivityRegistry, window: u32, document: &str) -> ActivityRecord {
        let (_, updates) = registry
            .dispatch_bridge_command(
                &command(
                    BEGIN_COMMAND,
                    window,
                    json!({ "name": "backup", "document": document }),
                ),
                "1",
            )
            .expect("activity command");
        updates[0].record().clone()
    }

    #[test]
    fn activities_keep_the_page_running_by_default() {
        let registry = ActivityRegistry::default();
        let record = begin(&registry, 7, "a");
        assert_eq!(record.name, "backup");
        assert!(record.keep_running);
        assert_eq!(record.window, Some(7));
    }

    #[test]
    fn pages_end_only_their_own_windows_activities() {
        let registry = ActivityRegistry::default();
        let record = begin(&registry, 7, "a");
        let end = |window| {
            registry
                .dispatch_bridge_command(
                    &command(END_COMMAND, window, json!({ "id": record.id })),
                    "1",
                )
                .expect("activity command")
        };
        assert!(end(8).1.is_empty());
        assert!(matches!(end(7).1[..], [ActivityHostUpdate::End(_)]));
        assert!(registry.list().is_empty());
    }

    #[test]
    fn a_new_document_ends_what_the_one_before_it_began() {
        let registry = ActivityRegistry::default();
        begin(&registry, 7, "before");
        let other_window = begin(&registry, 8, "elsewhere");
        let (_, updates) = registry
            .dispatch_bridge_command(
                &command(DOCUMENT_OPENED_COMMAND, 7, json!({ "document": "after" })),
                "1",
            )
            .expect("activity command");
        assert_eq!(updates.len(), 1);
        assert_eq!(registry.list(), vec![other_window]);
    }

    #[test]
    fn lease_emits_begin_and_end() {
        let counter = Arc::new(AtomicUsize::new(0));
        let emitter: Arc<dyn ActivityEventEmitter> = Arc::new(CountingEmitter(counter.clone()));
        let registry = ActivityRegistry::default();
        let lease = registry.lease(ActivityOptions::new("job"), Some(emitter));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        lease.end();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
