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
fn pages_end_only_their_own_browsers_activities() {
    let registry = ActivityRegistry::default();
    let record = begin(&registry, 7, "a");
    let end = |window, browser| {
        registry
            .dispatch_bridge_command(
                &command(END_COMMAND, window, json!({ "id": record.id })),
                browser,
            )
            .expect("activity command")
    };
    assert!(end(8, "1").1.is_empty());
    assert!(end(7, "2").1.is_empty());
    assert!(matches!(end(7, "1").1[..], [ActivityHostUpdate::End(_)]));
    assert!(registry.list().is_empty());
}

#[test]
fn a_window_keeps_a_bounded_number_of_page_activities() {
    let registry = ActivityRegistry::default();
    for _ in 0..MAX_PAGE_ACTIVITIES_PER_WINDOW {
        begin(&registry, 7, "a");
    }
    let (result, updates) = registry
        .dispatch_bridge_command(&command(BEGIN_COMMAND, 7, json!({ "document": "a" })), "1")
        .expect("activity command");
    assert!(result.is_err());
    assert!(updates.is_empty());
    begin(&registry, 8, "b");
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

/// Counts the machine kept awake while it lives.
struct Awake(Arc<AtomicUsize>);

impl Drop for Awake {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[test]
fn a_page_keeps_the_machine_awake_until_its_document_goes() {
    let awake = Arc::new(AtomicUsize::new(0));
    let kept = Arc::clone(&awake);
    let registry = ActivityRegistry::keeping_awake(move |keep_awake, reason| {
        assert_eq!((keep_awake, reason), (KeepAwake::Display, "Presenting"));
        kept.fetch_add(1, Ordering::SeqCst);
        KeepAwakeGuard::new(Awake(Arc::clone(&kept)))
    });
    let (_, updates) = registry
        .dispatch_bridge_command(
            &command(
                BEGIN_COMMAND,
                7,
                json!({ "name": "Presenting", "keepAwake": "display", "document": "a" }),
            ),
            "1",
        )
        .expect("activity command");
    assert_eq!(updates[0].record().keep_awake, Some(KeepAwake::Display));
    assert_eq!(awake.load(Ordering::SeqCst), 1);
    registry.dispatch_bridge_command(
        &command(DOCUMENT_CLOSED_COMMAND, 7, json!({ "document": "a" })),
        "1",
    );
    assert_eq!(awake.load(Ordering::SeqCst), 0);
}
