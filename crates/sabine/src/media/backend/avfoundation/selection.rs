use block2::RcBlock;
use objc2::rc::Retained;
use objc2_av_foundation::{
    AVMediaCharacteristic, AVMediaCharacteristicAudible, AVMediaCharacteristicLegible,
    AVMediaSelectionGroup, AVMediaSelectionOption, AVMediaTypeVideo, AVPlayerItem,
};
use objc2_foundation::NSError;

use super::observer::Target;
use crate::media::tracks::{Kind, Track, Tracks};

/// The audio and subtitle choices an item's asset offers.
#[derive(Default)]
pub(super) struct Groups {
    audio: Option<Retained<AVMediaSelectionGroup>>,
    subtitles: Option<Retained<AVMediaSelectionGroup>>,
    loaded: u8,
}

#[derive(Clone, Copy)]
pub(super) enum Group {
    Audio,
    Subtitles,
}

const GROUPS: [Group; 2] = [Group::Audio, Group::Subtitles];

/// A loaded selection group on its way to the main thread.
struct Loaded(Option<Retained<AVMediaSelectionGroup>>);

// SAFETY: AVFoundation's media selection groups are immutable.
unsafe impl Send for Loaded {}

impl Loaded {
    fn into_inner(self) -> Option<Retained<AVMediaSelectionGroup>> {
        self.0
    }
}

impl Group {
    fn characteristic(self) -> Option<&'static AVMediaCharacteristic> {
        match self {
            Self::Audio => unsafe { AVMediaCharacteristicAudible },
            Self::Subtitles => unsafe { AVMediaCharacteristicLegible },
        }
    }

    fn kind(self) -> Kind {
        match self {
            Self::Audio => Kind::Audio,
            Self::Subtitles => Kind::Subtitle,
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::Audio => "audio-",
            Self::Subtitles => "subtitle-",
        }
    }
}

impl Groups {
    /// Stores a loaded group, returning whether every group has loaded.
    pub(super) fn insert(
        &mut self,
        group: Group,
        loaded: Option<Retained<AVMediaSelectionGroup>>,
    ) -> bool {
        match group {
            Group::Audio => self.audio = loaded,
            Group::Subtitles => self.subtitles = loaded,
        }
        self.loaded += 1;
        usize::from(self.loaded) == GROUPS.len()
    }

    fn get(&self, group: Group) -> Option<&AVMediaSelectionGroup> {
        match group {
            Group::Audio => self.audio.as_deref(),
            Group::Subtitles => self.subtitles.as_deref(),
        }
    }
}

/// Loads the item's audio and subtitle groups, handing each to the playback.
pub(super) fn load(item: &AVPlayerItem, target: Target) {
    let asset = unsafe { item.asset() };
    for group in GROUPS {
        let Some(characteristic) = group.characteristic() else {
            target.deliver(move |playback| playback.group_loaded(group, None));
            continue;
        };
        let done = RcBlock::new(
            move |loaded: *mut AVMediaSelectionGroup, _error: *mut NSError| {
                let loaded = Loaded(unsafe { Retained::retain(loaded) });
                target.deliver(move |playback| playback.group_loaded(group, loaded.into_inner()));
            },
        );
        unsafe {
            asset.loadMediaSelectionGroupForMediaCharacteristic_completionHandler(
                characteristic,
                &done,
            )
        };
    }
}

/// Lists the item's video tracks and its audio and subtitle options.
pub(super) fn tracks(item: &AVPlayerItem, groups: &Groups) -> Vec<Track> {
    let video_type = unsafe { AVMediaTypeVideo };
    let video = unsafe { item.tracks() }
        .iter()
        .filter_map(|track| unsafe { track.assetTrack() })
        .filter(|track| video_type.is_some_and(|video| *unsafe { track.mediaType() } == *video))
        .map(|track| Track {
            id: format!("video-{}", unsafe { track.trackID() }),
            kind: Kind::Video,
            language: None,
            label: None,
            codec: None,
            preferred: true,
        })
        .collect::<Vec<_>>();
    let options = GROUPS.into_iter().flat_map(|group| {
        let Some(selection) = groups.get(group) else {
            return Vec::new();
        };
        let preferred = unsafe { selection.defaultOption() };
        unsafe { selection.options() }
            .iter()
            .enumerate()
            .map(|(index, option)| Track {
                id: format!("{}{index}", group.prefix()),
                kind: group.kind(),
                language: unsafe { option.extendedLanguageTag() }.map(|tag| tag.to_string()),
                label: Some(unsafe { option.displayName() }.to_string()),
                codec: unsafe { option.mediaSubTypes() }
                    .firstObject()
                    .and_then(|subtype| four_cc(subtype.unsignedIntValue())),
                preferred: preferred.as_deref() == Some(&*option),
            })
            .collect()
    });
    video.into_iter().chain(options).collect()
}

/// Selects the tracks the page chose.
pub(super) fn apply(item: &AVPlayerItem, groups: &Groups, tracks: &Tracks) {
    for (group, chosen) in [
        (Group::Audio, tracks.audio()),
        (Group::Subtitles, tracks.subtitle()),
    ] {
        if let Some(selection) = groups.get(group) {
            let option = chosen.and_then(|id| option(selection, group, id));
            unsafe { item.selectMediaOption_inMediaSelectionGroup(option.as_deref(), selection) };
        }
    }
}

/// The ids of the tracks the item is playing.
pub(super) fn selected(item: &AVPlayerItem, groups: &Groups) -> Vec<String> {
    let current = unsafe { item.currentMediaSelection() };
    let video = unsafe { item.tracks() }
        .iter()
        .filter(|track| unsafe { track.isEnabled() })
        .filter_map(|track| unsafe { track.assetTrack() })
        .map(|track| format!("video-{}", unsafe { track.trackID() }))
        .collect::<Vec<_>>();
    let options = GROUPS.into_iter().filter_map(|group| {
        let selection = groups.get(group)?;
        let chosen = unsafe { current.selectedMediaOptionInMediaSelectionGroup(selection) }?;
        unsafe { selection.options() }
            .iter()
            .position(|option| *option == *chosen)
            .map(|index| format!("{}{index}", group.prefix()))
    });
    video.into_iter().chain(options).collect()
}

fn option(
    selection: &AVMediaSelectionGroup,
    group: Group,
    id: &str,
) -> Option<Retained<AVMediaSelectionOption>> {
    let index = id.strip_prefix(group.prefix())?.parse().ok()?;
    let options = unsafe { selection.options() };
    (index < options.count()).then(|| options.objectAtIndex(index))
}

/// Core Media names codecs by a four-character code.
fn four_cc(code: u32) -> Option<String> {
    let bytes = code.to_be_bytes();
    bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b' ')
        .then(|| String::from_utf8_lossy(&bytes).trim().to_string())
}
