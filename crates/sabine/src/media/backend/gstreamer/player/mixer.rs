use std::{
    ffi::{CStr, c_int, c_void},
    sync::{Arc, Mutex},
};

use crate::media::backend::gstreamer::gst::{self, Gst, Handle, types};
use crate::media::command::AudioMix;

/// The `audiomixer` a `playbin3` plays several audio streams through, each
/// at its own volume. Each stream's input learns its volume when the stream
/// starts, so newly selected streams never play louder than asked.
pub(super) struct Mixer {
    pub(super) element: Handle,
    levels: Arc<Levels>,
}

struct Levels {
    gst: &'static Gst,
    mix: Mutex<Vec<AudioMix>>,
}

impl Mixer {
    /// # Safety
    /// `element` must be a live `audiomixer` that the pipeline keeps alive.
    pub(super) unsafe fn new(gst: &'static Gst, element: *mut c_void) -> Self {
        let levels = Arc::new(Levels {
            gst,
            mix: Mutex::new(Vec::new()),
        });
        let handler: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) = input_added;
        unsafe {
            (gst.g_signal_connect_data)(
                element,
                c"pad-added".as_ptr(),
                handler as *const c_void,
                Arc::into_raw(Arc::clone(&levels)) as *mut c_void,
                Some(release_signal),
                0,
            )
        };
        Self {
            element: Handle(element),
            levels,
        }
    }

    pub(super) fn set(&self, mix: &[AudioMix]) {
        let gst = self.levels.gst;
        let mut levels = self
            .levels
            .mix
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *levels == mix {
            return;
        }
        mix.clone_into(&mut levels);
        unsafe {
            let inputs = (gst.gst_element_iterate_sink_pads)(self.element.0);
            let mut input = types::GValue::default();
            loop {
                match (gst.gst_iterator_next)(inputs, &mut input) {
                    types::ITERATOR_OK => {
                        let pad = (gst.g_value_get_object)(&input);
                        let stream = gst::take_string(gst, (gst.gst_pad_get_stream_id)(pad));
                        if let Some(stream) = stream {
                            set_volume(gst, pad, volume(&levels, &stream));
                        }
                        (gst.g_value_reset)(&mut input);
                    }
                    types::ITERATOR_RESYNC => (gst.gst_iterator_resync)(inputs),
                    _ => break,
                }
            }
            if input.is_set() {
                (gst.g_value_unset)(&mut input);
            }
            (gst.gst_iterator_free)(inputs);
        }
    }
}

fn volume(mix: &[AudioMix], stream: &str) -> f64 {
    mix.iter()
        .find(|track| track.id == stream)
        .map_or(0.0, |track| track.volume)
}

unsafe fn set_volume(gst: &Gst, pad: *mut c_void, volume: f64) {
    unsafe { gst::set_property(gst, pad, c"volume", &gst::text(&volume.to_string())) };
}

unsafe extern "C" fn input_added(_mixer: *mut c_void, pad: *mut c_void, levels: *mut c_void) {
    let gst = unsafe { &*(levels as *const Levels) }.gst;
    unsafe {
        Arc::increment_strong_count(levels as *const Levels);
        (gst.gst_pad_add_probe)(
            pad,
            types::PAD_PROBE_TYPE_EVENT_DOWNSTREAM,
            Some(stream_started),
            levels,
            Some(release_probe),
        );
    }
}

unsafe extern "C" fn stream_started(
    pad: *mut c_void,
    info: *mut c_void,
    levels: *mut c_void,
) -> c_int {
    let levels = unsafe { &*(levels as *const Levels) };
    let gst = levels.gst;
    let event = unsafe { (gst.gst_pad_probe_info_get_event)(info) };
    if event.is_null()
        || unsafe { &*(event as *const types::EventHeader) }.kind != types::EVENT_STREAM_START
    {
        return types::PAD_PROBE_OK;
    }
    let mut stream = std::ptr::null();
    unsafe { (gst.gst_event_parse_stream_start)(event, &mut stream) };
    if !stream.is_null() {
        let stream = unsafe { CStr::from_ptr(stream) }.to_string_lossy();
        let mix = levels.mix.lock().unwrap_or_else(|error| error.into_inner());
        unsafe { set_volume(gst, pad, volume(&mix, &stream)) };
    }
    types::PAD_PROBE_OK
}

unsafe extern "C" fn release_probe(levels: *mut c_void) {
    drop(unsafe { Arc::from_raw(levels as *const Levels) });
}

unsafe extern "C" fn release_signal(levels: *mut c_void, _closure: *mut c_void) {
    drop(unsafe { Arc::from_raw(levels as *const Levels) });
}
