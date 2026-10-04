use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::c_void,
    rc::Weak,
    sync::atomic::{AtomicU64, Ordering},
};

use dispatch2::DispatchQueue;
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, define_class, msg_send, rc::Retained,
    runtime::AnyObject,
};
use objc2_av_foundation::{
    AVPlayerItemLegibleOutput, AVPlayerItemLegibleOutputPushDelegate,
    AVPlayerItemOutputPushDelegate,
};
use objc2_core_media::CMTime;
use objc2_foundation::{
    NSArray, NSAttributedString, NSNotification, NSObject, NSObjectProtocol, NSString,
};

use super::playback::{Change, Playback};

thread_local! {
    /// Live playbacks by id, reachable only on the main thread.
    static PLAYBACKS: RefCell<HashMap<u64, Weak<Playback>>> = RefCell::new(HashMap::new());
}

/// Identifies a playback from AVFoundation's callback threads.
#[derive(Clone, Copy)]
pub(super) struct Target(u64);

impl Target {
    pub(super) fn register(playback: Weak<Playback>, _main_thread: MainThreadMarker) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        PLAYBACKS.with_borrow_mut(|playbacks| playbacks.insert(id, playback));
        Self(id)
    }

    pub(super) fn unregister(self) {
        PLAYBACKS.with_borrow_mut(|playbacks| playbacks.remove(&self.0));
    }

    /// Runs `work` on the main thread as its own task, so AVFoundation's
    /// callbacks never re-enter a playback that is busy calling into it.
    pub(super) fn deliver(self, work: impl FnOnce(&Playback) + Send + 'static) {
        DispatchQueue::main().exec_async(move || {
            let playback =
                PLAYBACKS.with_borrow(|playbacks| playbacks.get(&self.0).and_then(Weak::upgrade));
            if let Some(playback) = playback {
                work(&playback);
            }
        });
    }
}

pub(super) struct Ivars {
    target: Target,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and `Observer` does
    // not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[ivars = Ivars]
    pub(super) struct Observer;

    impl Observer {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe_value(
            &self,
            key_path: Option<&NSString>,
            _object: Option<&AnyObject>,
            _change: Option<&AnyObject>,
            _context: *mut c_void,
        ) {
            if let Some(change) = key_path.and_then(|path| Change::from_key_path(&path.to_string())) {
                self.ivars().target.deliver(move |playback| playback.changed(change));
            }
        }

        #[unsafe(method(itemEnded:))]
        fn item_ended(&self, _notification: &NSNotification) {
            self.ivars().target.deliver(Playback::ended);
        }

        #[unsafe(method(selectionChanged:))]
        fn selection_changed(&self, _notification: &NSNotification) {
            self.ivars().target.deliver(Playback::selection_changed);
        }
    }

    unsafe impl NSObjectProtocol for Observer {}

    unsafe impl AVPlayerItemOutputPushDelegate for Observer {}

    unsafe impl AVPlayerItemLegibleOutputPushDelegate for Observer {
        #[unsafe(method(legibleOutput:didOutputAttributedStrings:nativeSampleBuffers:forItemTime:))]
        fn legible_output(
            &self,
            _output: &AVPlayerItemLegibleOutput,
            strings: &NSArray<NSAttributedString>,
            _native_samples: &NSArray,
            item_time: CMTime,
        ) {
            let text = strings
                .iter()
                .map(|string| string.string().to_string())
                .collect::<Vec<_>>()
                .join("\n");
            let start = unsafe { item_time.seconds() };
            self.ivars()
                .target
                .deliver(move |playback| playback.cue(text, start));
        }
    }
);

impl Observer {
    pub(super) fn new(target: Target) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { target });
        unsafe { msg_send![super(this), init] }
    }
}
