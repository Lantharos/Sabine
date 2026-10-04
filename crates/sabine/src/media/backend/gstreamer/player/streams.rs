use std::ffi::{CStr, c_void};

use crate::media::backend::gstreamer::gst::{self, Gst, types};
use crate::media::tracks::{Kind, Track, Tracks};

/// The streams that should play: the first video stream and the chosen
/// audio and subtitle streams.
pub(super) fn selection(tracks: &Tracks) -> Vec<String> {
    tracks
        .of(Kind::Video)
        .next()
        .map(|track| track.id.as_str())
        .into_iter()
        .chain(tracks.audio())
        .chain(tracks.subtitle())
        .map(str::to_string)
        .collect()
}

/// Reads the streams a stream-collection message offers.
///
/// # Safety
/// `message` must be a live stream-collection message.
pub(super) unsafe fn collection(gst: &Gst, message: *mut c_void) -> Option<Vec<Track>> {
    let mut collection = std::ptr::null_mut();
    unsafe { (gst.gst_message_parse_stream_collection)(message, &mut collection) };
    if collection.is_null() {
        return None;
    }
    let count = unsafe { (gst.gst_stream_collection_get_size)(collection) };
    let tracks = (0..count)
        .filter_map(|index| unsafe {
            read_track(
                gst,
                (gst.gst_stream_collection_get_stream)(collection, index),
            )
        })
        .collect();
    unsafe { (gst.gst_object_unref)(collection) };
    Some(tracks)
}

/// Reads which streams a streams-selected message reports as playing.
///
/// # Safety
/// `message` must be a live streams-selected message.
pub(super) unsafe fn selected(gst: &Gst, message: *mut c_void) -> Vec<String> {
    let count = unsafe { (gst.gst_message_streams_selected_get_size)(message) };
    (0..count)
        .filter_map(|index| unsafe {
            let stream = (gst.gst_message_streams_selected_get_stream)(message, index);
            let id = stream_id(gst, stream);
            (gst.gst_object_unref)(stream);
            id
        })
        .collect()
}

unsafe fn read_track(gst: &Gst, stream: *mut c_void) -> Option<Track> {
    let kind = match unsafe { (gst.gst_stream_get_stream_type)(stream) } {
        kind if kind & types::STREAM_TYPE_VIDEO != 0 => Kind::Video,
        kind if kind & types::STREAM_TYPE_AUDIO != 0 => Kind::Audio,
        kind if kind & types::STREAM_TYPE_TEXT != 0 => Kind::Subtitle,
        _ => return None,
    };
    let id = unsafe { stream_id(gst, stream) }?;
    let tags = unsafe { (gst.gst_stream_get_tags)(stream) };
    let tag = |name: &str| {
        if tags.is_null() {
            return None;
        }
        let name = gst::text(name);
        let mut value = std::ptr::null_mut();
        unsafe {
            (gst.gst_tag_list_get_string)(tags, name.as_ptr(), &mut value);
            gst::take_string(gst, value)
        }
    };
    let codec = match kind {
        Kind::Video => tag("video-codec"),
        Kind::Audio => tag("audio-codec"),
        Kind::Subtitle => tag("subtitle-codec"),
    }
    .or_else(|| tag("codec"));
    let track = Track {
        id,
        kind,
        language: tag("language-code"),
        label: tag("title"),
        codec,
        preferred: unsafe { (gst.gst_stream_get_stream_flags)(stream) } & types::STREAM_FLAG_SELECT
            != 0,
    };
    if !tags.is_null() {
        unsafe { (gst.gst_mini_object_unref)(tags) };
    }
    Some(track)
}

unsafe fn stream_id(gst: &Gst, stream: *mut c_void) -> Option<String> {
    let id = unsafe { (gst.gst_stream_get_stream_id)(stream) };
    (!id.is_null()).then(|| unsafe { CStr::from_ptr(id) }.to_string_lossy().into_owned())
}
