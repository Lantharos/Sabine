// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Stream attributes come back as PROPVARIANTs the caller must clear. Timed-text
// strings are borrowed from the engine and copied before the call returns.

use windows::Win32::{
    Media::MediaFoundation::{
        IMFMediaEngineEx, IMFTimedText, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SD_LANGUAGE,
        MF_SD_STREAM_NAME, MF_TIMED_TEXT_TRACK_KIND_CAPTIONS, MF_TIMED_TEXT_TRACK_KIND_SUBTITLES,
        MFMediaType_Audio, MFMediaType_Video,
    },
    System::{
        Com::StructuredStorage::{PROPVARIANT, PropVariantClear},
        Variant::{VT_CLSID, VT_LPWSTR},
    },
};
use windows::core::{GUID, PWSTR};

use crate::media::tracks::{Kind, Track, Tracks};

const VIDEO: &str = "video-";
const AUDIO: &str = "audio-";
const SUBTITLE: &str = "subtitle-";

/// Lists the engine's video and audio streams and its subtitle tracks.
pub(super) fn tracks(media: &IMFMediaEngineEx, text: Option<&IMFTimedText>) -> Vec<Track> {
    let count = unsafe { media.GetNumberOfStreams() }.unwrap_or_default();
    let streams = (0..count).filter_map(|index| {
        let (kind, prefix) =
            match attribute(media, index, &MF_MT_MAJOR_TYPE).and_then(Value::guid)? {
                major if major == MFMediaType_Video => (Kind::Video, VIDEO),
                major if major == MFMediaType_Audio => (Kind::Audio, AUDIO),
                _ => return None,
            };
        Some(Track {
            id: format!("{prefix}{index}"),
            kind,
            language: attribute(media, index, &MF_SD_LANGUAGE).and_then(Value::text),
            label: attribute(media, index, &MF_SD_STREAM_NAME).and_then(Value::text),
            codec: attribute(media, index, &MF_MT_SUBTYPE)
                .and_then(Value::guid)
                .and_then(|subtype| four_cc(subtype.data1)),
            preferred: false,
        })
    });
    streams.chain(subtitle_tracks(text)).collect()
}

fn subtitle_tracks(text: Option<&IMFTimedText>) -> Vec<Track> {
    let Some(list) = text.and_then(|text| unsafe { text.GetTextTracks() }.ok()) else {
        return Vec::new();
    };
    (0..unsafe { list.GetLength() })
        .filter_map(|index| unsafe { list.GetTrack(index) }.ok())
        .filter(|track| {
            matches!(
                unsafe { track.GetTrackKind() },
                MF_TIMED_TEXT_TRACK_KIND_SUBTITLES | MF_TIMED_TEXT_TRACK_KIND_CAPTIONS
            )
        })
        .map(|track| unsafe {
            Track {
                id: format!("{SUBTITLE}{}", track.GetId()),
                kind: Kind::Subtitle,
                language: track.GetLanguage().ok().and_then(borrowed),
                label: track.GetLabel().ok().and_then(borrowed),
                codec: None,
                preferred: false,
            }
        })
        .collect()
}

/// Plays the chosen audio stream and shows the chosen subtitle track.
pub(super) fn apply(media: &IMFMediaEngineEx, text: Option<&IMFTimedText>, tracks: &Tracks) {
    let audio = tracks.audio().and_then(|id| index(id, AUDIO));
    for track in tracks.of(Kind::Audio) {
        if let Some(stream) = index(&track.id, AUDIO) {
            let _ = unsafe { media.SetStreamSelection(stream, Some(stream) == audio) };
        }
    }
    let _ = unsafe { media.ApplyStreamSelections() };
    if let Some(text) = text {
        let subtitle = tracks.subtitle().and_then(|id| index(id, SUBTITLE));
        for track in tracks.of(Kind::Subtitle) {
            if let Some(id) = index(&track.id, SUBTITLE) {
                let _ = unsafe { text.SelectTrack(id, Some(id) == subtitle) };
            }
        }
    }
}

/// The ids of the streams and subtitle tracks playing.
pub(super) fn selected(media: &IMFMediaEngineEx, tracks: &Tracks) -> Vec<String> {
    let streams = [Kind::Video, Kind::Audio]
        .into_iter()
        .flat_map(|kind| tracks.of(kind))
        .filter(|track| {
            let prefix = if track.kind == Kind::Video {
                VIDEO
            } else {
                AUDIO
            };
            index(&track.id, prefix).is_some_and(|stream| {
                unsafe { media.GetStreamSelection(stream) }.is_ok_and(|selected| selected.as_bool())
            })
        });
    streams
        .map(|track| track.id.clone())
        .chain(tracks.subtitle().map(str::to_string))
        .collect()
}

fn index(id: &str, prefix: &str) -> Option<u32> {
    id.strip_prefix(prefix)?.parse().ok()
}

/// A stream attribute, cleared when dropped.
struct Value(PROPVARIANT);

impl Value {
    fn guid(self) -> Option<GUID> {
        let value = unsafe { &self.0.Anonymous.Anonymous };
        (value.vt == VT_CLSID && !unsafe { value.Anonymous.puuid }.is_null())
            .then(|| unsafe { *value.Anonymous.puuid })
    }

    fn text(self) -> Option<String> {
        let value = unsafe { &self.0.Anonymous.Anonymous };
        if value.vt != VT_LPWSTR {
            return None;
        }
        borrowed(unsafe { value.Anonymous.pwszVal })
    }
}

impl Drop for Value {
    fn drop(&mut self) {
        let _ = unsafe { PropVariantClear(&mut self.0) };
    }
}

fn attribute(media: &IMFMediaEngineEx, stream: u32, key: &GUID) -> Option<Value> {
    unsafe { media.GetStreamAttribute(stream, key) }
        .ok()
        .map(Value)
}

fn borrowed(text: PWSTR) -> Option<String> {
    (!text.is_null())
        .then(|| unsafe { text.to_string() }.ok())
        .flatten()
        .filter(|text| !text.is_empty())
}

/// Media Foundation names most codecs by a four-character code.
fn four_cc(code: u32) -> Option<String> {
    let bytes = code.to_le_bytes();
    bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b' ')
        .then(|| String::from_utf8_lossy(&bytes).trim().to_string())
}
