//! The workspace's notification center reports sleep and wake, the
//! distributed notification center the screen locking, and IOKit's power
//! sources the power source, all on the main thread's run loop.

mod sources;

use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{
    NSWorkspace, NSWorkspaceDidWakeNotification, NSWorkspaceWillSleepNotification,
};
use objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSNotificationCenter, NSNotificationName,
    ns_string,
};

use super::Reporter;

type Observer = Retained<ProtocolObject<dyn NSObjectProtocol>>;

pub(super) struct Watcher {
    workspace: Vec<Observer>,
    distributed: Vec<Observer>,
    _sources: sources::Sources,
}

// SAFETY: the observers are only handed back to their notification centers,
// which take observers off from any thread.
unsafe impl Send for Watcher {}
unsafe impl Sync for Watcher {}

impl Watcher {
    pub(super) fn start(reporter: Reporter) -> Self {
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        let distributed = NSDistributedNotificationCenter::defaultCenter();
        Self {
            workspace: vec![
                observe(&workspace, unsafe { NSWorkspaceWillSleepNotification }, {
                    let reporter = reporter.clone();
                    move || reporter.suspending()
                }),
                observe(&workspace, unsafe { NSWorkspaceDidWakeNotification }, {
                    let reporter = reporter.clone();
                    move || reporter.resumed()
                }),
            ],
            distributed: vec![
                observe(&distributed, ns_string!("com.apple.screenIsLocked"), {
                    let reporter = reporter.clone();
                    move || reporter.locked(true)
                }),
                observe(&distributed, ns_string!("com.apple.screenIsUnlocked"), {
                    let reporter = reporter.clone();
                    move || reporter.locked(false)
                }),
            ],
            _sources: sources::Sources::watch(reporter),
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
        let distributed = NSDistributedNotificationCenter::defaultCenter();
        for observer in self.workspace.drain(..) {
            unsafe { workspace.removeObserver(AsRef::<AnyObject>::as_ref(&*observer)) };
        }
        for observer in self.distributed.drain(..) {
            unsafe { distributed.removeObserver(AsRef::<AnyObject>::as_ref(&*observer)) };
        }
    }
}

fn observe(
    center: &NSNotificationCenter,
    name: &NSNotificationName,
    heard: impl Fn() + 'static,
) -> Observer {
    let block = RcBlock::new(move |_: NonNull<NSNotification>| heard());
    unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) }
}
