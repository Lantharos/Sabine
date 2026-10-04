use std::{
    ffi::{CStr, c_int, c_void},
    sync::{Arc, OnceLock},
};

use super::cues::cue_ready;
use super::inbox::{Inbox, Message};
use crate::media::backend::gstreamer::gst::{self, CudaContext, Gst, Handle, types};

const PLAYBIN_FLAGS: &str = "video+audio+text+soft-volume+native-video";
const VIDEO_SINK: &str = "glupload ! glcolorconvert ! appsink name=pictures \
     caps=video/x-raw(memory:GLMemory),format=RGBA,texture-target=2D \
     max-buffers=1 drop=true sync=true enable-last-sample=false";

/// A `playbin3` whose video lands in GL textures the presenter samples and
/// whose subtitles arrive as text instead of being burnt into the picture.
pub(super) struct Pipeline {
    gst: &'static Gst,
    pub(super) playbin: Handle,
    pub(super) pictures: Handle,
    inbox: Arc<Inbox>,
}

struct BusBridge {
    gst: &'static Gst,
    inbox: Arc<Inbox>,
    playbin: Handle,
    gl_display: Handle,
    gl_context: Handle,
    cuda: OnceLock<Option<CudaContext>>,
}

impl Pipeline {
    pub(super) fn new(
        gst: &'static Gst,
        uri: &str,
        gl_display: Handle,
        gl_context: Handle,
        inbox: &Arc<Inbox>,
    ) -> Result<Self, String> {
        unsafe {
            let playbin = make(gst, "playbin3")?;
            let (video, subtitles) = match sinks(gst) {
                Ok(sinks) => sinks,
                Err(error) => {
                    (gst.gst_object_unref)(playbin);
                    return Err(error);
                }
            };
            let pictures_name = gst::text("pictures");
            let pictures = (gst.gst_bin_get_by_name)(video, pictures_name.as_ptr());
            gst::set_property(gst, subtitles, "caps", "text/x-raw");
            gst::set_property(gst, subtitles, "sync", "true");
            let element_type = (gst.gst_element_get_type)();
            gst::set_object_property(gst, playbin, "video-sink", element_type, video);
            gst::set_object_property(gst, playbin, "text-sink", element_type, subtitles);
            if let Ok(tempo) = make(gst, "scaletempo") {
                gst::set_object_property(gst, playbin, "audio-filter", element_type, tempo);
            }
            gst::set_property(gst, playbin, "flags", PLAYBIN_FLAGS);
            gst::set_property(gst, playbin, "uri", uri);

            let mut picture_callbacks =
                types::AppSinkCallbacks::samples(Some(picture_ready), picture_ready);
            (gst.gst_app_sink_set_callbacks)(
                pictures,
                &mut picture_callbacks,
                Arc::into_raw(Arc::clone(inbox)) as *mut c_void,
                Some(release_inbox),
            );
            let mut cue_callbacks = types::AppSinkCallbacks::samples(None, cue_ready);
            (gst.gst_app_sink_set_callbacks)(
                subtitles,
                &mut cue_callbacks,
                Arc::into_raw(Arc::clone(inbox)) as *mut c_void,
                Some(release_inbox),
            );

            let bus = (gst.gst_element_get_bus)(playbin);
            let bridge = Box::new(BusBridge {
                gst,
                inbox: Arc::clone(inbox),
                playbin: Handle(playbin),
                gl_display,
                gl_context,
                cuda: OnceLock::new(),
            });
            (gst.gst_bus_set_sync_handler)(
                bus,
                Some(bus_message),
                Box::into_raw(bridge).cast(),
                Some(release_bridge),
            );
            (gst.gst_object_unref)(bus);
            Ok(Self {
                gst,
                playbin: Handle(playbin),
                pictures: Handle(pictures),
                inbox: Arc::clone(inbox),
            })
        }
    }

    pub(super) fn set_playing(&self, playing: bool) {
        let state = if playing {
            types::STATE_PLAYING
        } else {
            types::STATE_PAUSED
        };
        unsafe { (self.gst.gst_element_set_state)(self.playbin.0, state) };
    }

    pub(super) fn position(&self) -> Option<f64> {
        let mut position = 0;
        (unsafe {
            (self.gst.gst_element_query_position)(self.playbin.0, types::FORMAT_TIME, &mut position)
        } != 0
            && position >= 0)
            .then(|| position as f64 / types::SECOND)
    }

    pub(super) fn duration(&self) -> Option<f64> {
        let mut duration = 0;
        (unsafe {
            (self.gst.gst_element_query_duration)(self.playbin.0, types::FORMAT_TIME, &mut duration)
        } != 0
            && duration > 0)
            .then(|| duration as f64 / types::SECOND)
    }

    pub(super) fn seek(&self, time: f64, fast: bool, rate: f64) -> bool {
        let precision = if fast {
            types::SEEK_FLAG_KEY_UNIT | types::SEEK_FLAG_SNAP_NEAREST
        } else {
            types::SEEK_FLAG_ACCURATE
        };
        unsafe {
            (self.gst.gst_element_seek)(
                self.playbin.0,
                rate,
                types::FORMAT_TIME,
                types::SEEK_FLAG_FLUSH | precision,
                types::SEEK_TYPE_SET,
                (time.max(0.0) * types::SECOND) as i64,
                types::SEEK_TYPE_NONE,
                -1,
            ) != 0
        }
    }

    pub(super) fn change_rate(&self, rate: f64) -> bool {
        unsafe {
            (self.gst.gst_element_seek)(
                self.playbin.0,
                rate,
                types::FORMAT_TIME,
                types::SEEK_FLAG_INSTANT_RATE_CHANGE,
                types::SEEK_TYPE_NONE,
                -1,
                types::SEEK_TYPE_NONE,
                -1,
            ) != 0
        }
    }

    pub(super) fn set_volume(&self, volume: f64) {
        unsafe {
            gst::set_property(
                self.gst,
                self.playbin.0,
                "volume",
                &volume.clamp(0.0, 1.0).to_string(),
            )
        };
    }

    pub(super) fn set_muted(&self, muted: bool) {
        unsafe { gst::set_property(self.gst, self.playbin.0, "mute", &muted.to_string()) };
    }

    pub(super) fn select_streams(&self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let ids = ids.iter().map(|id| gst::text(id)).collect::<Vec<_>>();
        unsafe {
            let list = ids.iter().fold(std::ptr::null_mut(), |list, id| {
                (self.gst.g_list_append)(list, id.as_ptr() as *mut c_void)
            });
            let event = (self.gst.gst_event_new_select_streams)(list);
            (self.gst.g_list_free)(list);
            (self.gst.gst_element_send_event)(self.playbin.0, event);
        }
    }
}

impl Drop for Pipeline {
    /// Stops streaming and drops the messages that still reference the
    /// pipeline, so it is freed before the CUDA context its decoder used.
    fn drop(&mut self) {
        unsafe { (self.gst.gst_element_set_state)(self.playbin.0, types::STATE_NULL) };
        self.inbox.drop_messages();
        unsafe {
            (self.gst.gst_object_unref)(self.pictures.0);
            (self.gst.gst_object_unref)(self.playbin.0);
        }
    }
}

unsafe fn sinks(gst: &Gst) -> Result<(*mut c_void, *mut c_void), String> {
    let video = unsafe { parse_bin(gst, VIDEO_SINK) }?;
    match unsafe { make(gst, "appsink") } {
        Ok(subtitles) => Ok((video, subtitles)),
        Err(error) => {
            unsafe { (gst.gst_object_unref)(video) };
            Err(error)
        }
    }
}

unsafe fn make(gst: &Gst, factory: &str) -> Result<*mut c_void, String> {
    let name = gst::text(factory);
    let element = unsafe { (gst.gst_element_factory_make)(name.as_ptr(), std::ptr::null()) };
    if element.is_null() {
        Err(format!("the GStreamer {factory} element is not installed"))
    } else {
        Ok(element)
    }
}

unsafe fn parse_bin(gst: &Gst, description: &str) -> Result<*mut c_void, String> {
    let description = gst::text(description);
    let mut error = std::ptr::null_mut();
    let bin = unsafe {
        (gst.gst_parse_bin_from_description)(description.as_ptr(), types::TRUE, &mut error)
    };
    if bin.is_null() {
        Err(unsafe { gst::take_error(gst, error) })
    } else {
        Ok(bin)
    }
}

unsafe extern "C" fn picture_ready(_sink: *mut c_void, inbox: *mut c_void) -> c_int {
    let inbox = unsafe { &*(inbox as *const Inbox) };
    inbox.post(|mail| mail.picture = true);
    types::FLOW_OK
}

unsafe extern "C" fn release_inbox(inbox: *mut c_void) {
    drop(unsafe { Arc::from_raw(inbox as *const Inbox) });
}

unsafe extern "C" fn release_bridge(bridge: *mut c_void) {
    drop(unsafe { Box::from_raw(bridge as *mut BusBridge) });
}

/// Takes every bus message: the ones the player acts on go to its inbox,
/// the rest are released here.
unsafe extern "C" fn bus_message(
    _bus: *mut c_void,
    message: *mut c_void,
    bridge: *mut c_void,
) -> c_int {
    let bridge = unsafe { &*(bridge as *const BusBridge) };
    let gst = bridge.gst;
    let header = unsafe { &*(message as *const types::MessageHeader) };
    match header.kind {
        types::MESSAGE_STATE_CHANGED if header.source != bridge.playbin.0 => {}
        types::MESSAGE_STATE_CHANGED
        | types::MESSAGE_EOS
        | types::MESSAGE_ERROR
        | types::MESSAGE_BUFFERING
        | types::MESSAGE_DURATION_CHANGED
        | types::MESSAGE_ASYNC_DONE
        | types::MESSAGE_STREAM_COLLECTION
        | types::MESSAGE_STREAMS_SELECTED => {
            bridge
                .inbox
                .post(|mail| mail.messages.push(Message::new(gst, Handle(message))));
            return types::BUS_DROP;
        }
        types::MESSAGE_NEED_CONTEXT => unsafe { answer_context(bridge, message, header.source) },
        _ => {}
    }
    unsafe { (gst.gst_mini_object_unref)(message) };
    types::BUS_DROP
}

unsafe fn answer_context(bridge: &BusBridge, message: *mut c_void, element: *mut c_void) {
    let gst = bridge.gst;
    let mut context_type = std::ptr::null();
    if unsafe { (gst.gst_message_parse_context_type)(message, &mut context_type) } == 0 {
        return;
    }
    let context_type = unsafe { CStr::from_ptr(context_type) }.to_string_lossy();
    let context = match context_type.as_ref() {
        "gst.gl.GLDisplay" => unsafe {
            let context =
                (gst.gst_context_new)(gst::text("gst.gl.GLDisplay").as_ptr(), types::TRUE);
            (gst.gst_context_set_gl_display)(context, bridge.gl_display.0);
            Handle(context)
        },
        "gst.gl.app_context" => unsafe {
            let name = gst::text("gst.gl.app_context");
            let context = (gst.gst_context_new)(name.as_ptr(), types::TRUE);
            let structure = (gst.gst_context_writable_structure)(context);
            let mut value = types::GValue::default();
            (gst.g_value_init)(&mut value, (gst.gst_gl_context_get_type)());
            (gst.g_value_set_object)(&mut value, bridge.gl_context.0);
            (gst.gst_structure_set_value)(structure, gst::text("context").as_ptr(), &value);
            (gst.g_value_unset)(&mut value);
            Handle(context)
        },
        gst::CUDA_CONTEXT_TYPE => {
            if let Some(cuda) = bridge.cuda.get_or_init(|| CudaContext::create(gst)) {
                unsafe { (gst.gst_element_set_context)(element, cuda.context.0) };
            }
            return;
        }
        _ => return,
    };
    unsafe {
        (gst.gst_element_set_context)(element, context.0);
        (gst.gst_mini_object_unref)(context.0);
    }
}
