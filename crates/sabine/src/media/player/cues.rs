use std::ffi::{CStr, c_int, c_void};

use super::inbox::{Cue, Inbox};
use crate::media::gst::{self, Gst, types};

/// Subtitle text arriving at its presentation time on the text sink.
pub(super) unsafe extern "C" fn cue_ready(sink: *mut c_void, inbox: *mut c_void) -> c_int {
    let inbox = unsafe { &*(inbox as *const Inbox) };
    let Ok(gst) = gst::gst() else {
        return types::FLOW_OK;
    };
    unsafe {
        let sample = (gst.gst_app_sink_try_pull_sample)(sink, 0);
        if sample.is_null() {
            return types::FLOW_OK;
        }
        if let Some(cue) = read_cue(gst, sample) {
            inbox.post(|mail| mail.cues.push(cue));
        }
        (gst.gst_mini_object_unref)(sample);
    }
    types::FLOW_OK
}

unsafe fn read_cue(gst: &Gst, sample: *mut c_void) -> Option<Cue> {
    let buffer = unsafe { (gst.gst_sample_get_buffer)(sample) };
    if buffer.is_null() {
        return None;
    }
    let header = unsafe { &*(buffer as *const types::BufferHeader) };
    if header.pts == types::CLOCK_TIME_NONE {
        return None;
    }
    let mut map = types::MapInfo::default();
    if unsafe { (gst.gst_buffer_map)(buffer, &mut map, types::MAP_READ) } == 0 {
        return None;
    }
    let raw = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(map.data, map.size) })
        .trim_end_matches('\0')
        .to_string();
    unsafe { (gst.gst_buffer_unmap)(buffer, &mut map) };
    let text = if unsafe { is_markup(gst, sample) } {
        plain_text(&raw)
    } else {
        raw
    };
    let start = header.pts as f64 / types::SECOND;
    let duration = if header.duration == types::CLOCK_TIME_NONE {
        0.0
    } else {
        header.duration as f64 / types::SECOND
    };
    Some(Cue {
        text: text.trim().to_string(),
        start,
        end: start + duration,
    })
}

unsafe fn is_markup(gst: &Gst, sample: *mut c_void) -> bool {
    let name = gst::text("format");
    unsafe {
        let structure = (gst.gst_caps_get_structure)((gst.gst_sample_get_caps)(sample), 0);
        let format = (gst.gst_structure_get_string)(structure, name.as_ptr());
        !format.is_null() && CStr::from_ptr(format).to_bytes() == b"pango-markup"
    }
}

/// Drops Pango markup tags and decodes its entities.
fn plain_text(markup: &str) -> String {
    let mut text = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(start) = rest.find(['<', '&']) {
        text.push_str(&rest[..start]);
        rest = &rest[start..];
        if rest.starts_with('<') {
            rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
            continue;
        }
        let entity = rest.find(';').map_or(rest, |end| &rest[..=end]);
        text.push_str(match entity {
            "&amp;" => "&",
            "&lt;" => "<",
            "&gt;" => ">",
            "&quot;" => "\"",
            "&apos;" => "'",
            _ => entity,
        });
        rest = &rest[entity.len()..];
    }
    text.push_str(rest);
    text
}

#[cfg(test)]
mod tests {
    use super::plain_text;

    #[test]
    fn strips_markup_and_decodes_entities() {
        assert_eq!(
            plain_text("Second <i>italic</i> line &amp; <b>more</b> &lt;3"),
            "Second italic line & more <3"
        );
    }
}
