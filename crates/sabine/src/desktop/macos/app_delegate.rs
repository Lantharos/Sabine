use std::cell::RefCell;

use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject,
};
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSURL};
use sabine_platform::{PlatformEvent, SingleInstanceActivation, SingleInstancePolicy};

use super::UiQueue;
use crate::desktop::EventQueue;

thread_local! {
    static DELEGATE: RefCell<Option<Retained<ApplicationDelegate>>> = const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabineApplicationDelegate"]
    #[ivars = EventQueue]
    struct ApplicationDelegate;

    unsafe impl NSObjectProtocol for ApplicationDelegate {}

    unsafe impl NSApplicationDelegate for ApplicationDelegate {
        #[unsafe(method(application:openURLs:))]
        #[allow(non_snake_case)]
        fn application_openURLs(&self, _application: &NSApplication, urls: &NSArray<NSURL>) {
            let urls = urls
                .iter()
                .filter_map(|url| url.absoluteString().map(|url| url.to_string()))
                .collect::<Vec<_>>();
            if !urls.is_empty() {
                let _ = self.ivars().send(PlatformEvent::OpenUrls(urls));
            }
        }

        // The Dock icon or Finder opened the running app again: bring its
        // window back, as a second launch would.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn application_should_handle_reopen(
            &self,
            _application: &NSApplication,
            _has_visible_windows: bool,
        ) -> bool {
            let _ = self.ivars().send(PlatformEvent::SingleInstance(
                SingleInstanceActivation::new(SingleInstancePolicy::FocusExisting, Vec::new()),
            ));
            true
        }
    }
);

/// Delivers the URLs, documents and reopen requests macOS sends the app.
pub(in crate::desktop) struct AppEvents;

impl AppEvents {
    /// Installs the delegate right away on the main thread, so URLs that
    /// launched the app are not missed, and otherwise once the main thread
    /// runs its event loop.
    pub(in crate::desktop) fn install(events: EventQueue) -> Self {
        match MainThreadMarker::new() {
            Some(main_thread) => install(main_thread, events),
            None => UiQueue.run(move || install(main_queue_thread(), events)),
        }
        Self
    }
}

impl Drop for AppEvents {
    fn drop(&mut self) {
        UiQueue.run(|| {
            NSApplication::sharedApplication(main_queue_thread()).setDelegate(None);
            DELEGATE.with(|delegate| delegate.borrow_mut().take());
        });
    }
}

fn install(main_thread: MainThreadMarker, events: EventQueue) {
    let application = NSApplication::sharedApplication(main_thread);
    let delegate: Retained<ApplicationDelegate> = unsafe {
        msg_send![
            super(ApplicationDelegate::alloc(main_thread).set_ivars(events)),
            init
        ]
    };
    application.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    DELEGATE.with(|current| *current.borrow_mut() = Some(delegate));
}

fn main_queue_thread() -> MainThreadMarker {
    // SAFETY: tasks on the main dispatch queue run on the main thread.
    unsafe { MainThreadMarker::new_unchecked() }
}
