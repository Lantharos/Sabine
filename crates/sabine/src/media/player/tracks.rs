use std::ffi::{CStr, c_void};

use serde_json::{Value, json};

use super::inbox::TrackRequest;
use crate::media::gst::{self, Gst, types};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Video,
    Audio,
    Subtitle,
}

struct Track {
    id: String,
    kind: Kind,
    language: Option<String>,
    label: Option<String>,
    codec: Option<String>,
    preferred: bool,
}

/// The streams a file offers and which of them should play.
#[derive(Default)]
pub(super) struct Tracks {
    tracks: Vec<Track>,
    audio: Option<String>,
    subtitle: Option<String>,
    requested: TrackRequest,
    selected: Vec<String>,
}

impl Tracks {
    /// Reads a stream collection message, returning the streams to select.
    ///
    /// # Safety
    /// `message` must be a live stream-collection message.
    pub(super) unsafe fn collect(&mut self, gst: &Gst, message: *mut c_void) -> Vec<String> {
        let mut collection = std::ptr::null_mut();
        unsafe { (gst.gst_message_parse_stream_collection)(message, &mut collection) };
        if collection.is_null() {
            return self.selection();
        }
        let count = unsafe { (gst.gst_stream_collection_get_size)(collection) };
        self.tracks = (0..count)
            .filter_map(|index| unsafe {
                read_track(
                    gst,
                    (gst.gst_stream_collection_get_stream)(collection, index),
                )
            })
            .collect();
        unsafe { (gst.gst_object_unref)(collection) };
        self.audio = self.pick(Kind::Audio, self.requested.audio.clone());
        self.subtitle = self.pick(
            Kind::Subtitle,
            self.requested.subtitle.clone().or(Some(None)),
        );
        self.selection()
    }

    /// Applies the page's choice, returning the streams to select once known.
    pub(super) fn request(&mut self, request: TrackRequest) -> Option<Vec<String>> {
        if let Some(audio) = request.audio {
            self.requested.audio = Some(audio.clone());
            self.audio = self.pick(Kind::Audio, Some(audio));
        }
        if let Some(subtitle) = request.subtitle {
            self.requested.subtitle = Some(subtitle.clone());
            self.subtitle = self.pick(Kind::Subtitle, Some(subtitle));
        }
        (!self.tracks.is_empty()).then(|| self.selection())
    }

    /// Records which streams are playing.
    ///
    /// # Safety
    /// `message` must be a live streams-selected message.
    pub(super) unsafe fn selected(&mut self, gst: &Gst, message: *mut c_void) {
        let count = unsafe { (gst.gst_message_streams_selected_get_size)(message) };
        self.selected = (0..count)
            .filter_map(|index| unsafe {
                let stream = (gst.gst_message_streams_selected_get_stream)(message, index);
                let id = stream_id(gst, stream);
                (gst.gst_object_unref)(stream);
                id
            })
            .collect();
    }

    pub(super) fn subtitles_enabled(&self) -> bool {
        self.subtitle.is_some()
    }

    pub(super) fn to_json(&self) -> Value {
        let list = |kind: Kind| {
            self.tracks
                .iter()
                .filter(|track| track.kind == kind)
                .map(|track| {
                    json!({
                        "id": track.id,
                        "language": track.language,
                        "label": track.label,
                        "codec": track.codec,
                        "selected": self.selected.contains(&track.id),
                    })
                })
                .collect::<Vec<_>>()
        };
        json!({
            "video": list(Kind::Video),
            "audio": list(Kind::Audio),
            "subtitles": list(Kind::Subtitle),
        })
    }

    fn pick(&self, kind: Kind, requested: Option<Option<String>>) -> Option<String> {
        let mut candidates = self.tracks.iter().filter(|track| track.kind == kind);
        match requested {
            Some(Some(id)) => candidates.find(|track| track.id == id),
            Some(None) => None,
            None => {
                let candidates = candidates.collect::<Vec<_>>();
                candidates
                    .iter()
                    .find(|track| track.preferred)
                    .or(candidates.first())
                    .copied()
            }
        }
        .map(|track| track.id.clone())
    }

    fn selection(&self) -> Vec<String> {
        self.tracks
            .iter()
            .find(|track| track.kind == Kind::Video)
            .map(|track| track.id.clone())
            .into_iter()
            .chain(self.audio.clone())
            .chain(self.subtitle.clone())
            .collect()
    }
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
