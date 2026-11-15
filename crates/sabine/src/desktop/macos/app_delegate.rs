use std::{cell::RefCell, ffi::OsString};

use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject, sel,
};
use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSMenu, NSMenuItem};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString, NSURL, ns_string};
use sabine_platform::{PlatformEvent, SingleInstanceActivation, SingleInstancePolicy};
use sabine_service::manifest::AppAction;

use super::{UiQueue, main_queue_thread};
use crate::desktop::EventQueue;

thread_local! {
    static DELEGATE: RefCell<Option<Retained<ApplicationDelegate>>> = const { RefCell::new(None) };
}

/// What the delegate hands on, and the actions its Dock menu lists.
pub(in crate::desktop) struct AppEventTargets {
    pub(in crate::desktop) events: EventQueue,
    pub(in crate::desktop) actions: Vec<AppAction>,
    /// How an action reaches the app, as a second launch with the action's
    /// arguments would.
    pub(in crate::desktop) policy: SingleInstancePolicy,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabineApplicationDelegate"]
    #[ivars = AppEventTargets]
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
                let _ = self.ivars().events.send(PlatformEvent::OpenUrls(urls));
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
            let _ = self.ivars().events.send(PlatformEvent::SingleInstance(
                SingleInstanceActivation::new(SingleInstancePolicy::FocusExisting, Vec::new()),
            ));
            true
        }

        #[unsafe(method_id(applicationDockMenu:))]
        fn application_dock_menu(&self, _application: &NSApplication) -> Option<Retained<NSMenu>> {
            let actions = &self.ivars().actions;
            (!actions.is_empty()).then(|| {
                let menu = NSMenu::new(self.mtm());
                for (index, action) in actions.iter().enumerate() {
                    let item = unsafe {
                        NSMenuItem::initWithTitle_action_keyEquivalent(
                            NSMenuItem::alloc(self.mtm()),
                            &NSString::from_str(&action.name),
                            Some(sel!(runAppAction:)),
                            ns_string!(""),
                        )
                    };
                    unsafe { item.setTarget(Some(&***self)) };
                    item.setTag(index as isize);
                    menu.addItem(&item);
                }
                menu
            })
        }
    }

    impl ApplicationDelegate {
        #[unsafe(method(runAppAction:))]
        fn run_app_action(&self, item: &NSMenuItem) {
            let targets = self.ivars();
            let Some(action) = usize::try_from(item.tag())
                .ok()
                .and_then(|index| targets.actions.get(index))
            else {
                return;
            };
            let arguments = std::env::args_os()
                .take(1)
                .chain(action.arguments.iter().map(OsString::from))
                .collect();
            let _ = targets.events.send(PlatformEvent::SingleInstance(
                SingleInstanceActivation::new(targets.policy, arguments),
            ));
        }
    }
);

/// Delivers the URLs, documents, reopen requests and Dock menu actions macOS
/// sends the app.
pub(in crate::desktop) struct AppEvents;

impl AppEvents {
    /// Installs the delegate right away on the main thread, so URLs that
    /// launched the app are not missed, and otherwise once the main thread
    /// runs its event loop.
    pub(in crate::desktop) fn install(targets: AppEventTargets) -> Self {
        match MainThreadMarker::new() {
            Some(main_thread) => install(main_thread, targets),
            None => UiQueue.run(move || install(main_queue_thread(), targets)),
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

fn install(main_thread: MainThreadMarker, targets: AppEventTargets) {
    let application = NSApplication::sharedApplication(main_thread);
    let delegate: Retained<ApplicationDelegate> = unsafe {
        msg_send![
            super(ApplicationDelegate::alloc(main_thread).set_ivars(targets)),
            init
        ]
    };
    application.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    DELEGATE.with(|current| *current.borrow_mut() = Some(delegate));
}
