//! Text as assistive technology reads and edits it: Chromium's inline text
//! boxes become text runs, whose characters it counts in UTF-16 code units,
//! and the caret and selection become positions in those runs.

use accesskit::{Node, Role, TextDirection, TextPosition, TextSelection};

use super::frame::FrameTree;
use super::node_id;
use crate::osr::protocol::accessibility::{AxNode, attribute, state};

pub(super) fn set_text_run(target: &mut Node, data: &AxNode, text: &str) {
    let mut lengths = Vec::with_capacity(text.len());
    let mut unit_characters = Vec::with_capacity(text.len());
    let mut character_ends = Vec::with_capacity(text.len());
    for (index, character) in text.chars().enumerate() {
        lengths.push(character.len_utf8() as u8);
        unit_characters.extend(std::iter::repeat_n(index, character.len_utf16()));
        character_ends.push(unit_characters.len());
    }
    let offsets = data.ints(attribute::CHARACTER_OFFSETS);
    if offsets.len() == unit_characters.len() {
        let mut start = 0.0;
        let (positions, widths): (Vec<f32>, Vec<f32>) = character_ends
            .iter()
            .map(|end| {
                let position = start;
                start = offsets[end - 1] as f32;
                (position, start - position)
            })
            .unzip();
        target.set_character_positions(positions);
        target.set_character_widths(widths);
    }
    let mut word_starts: Vec<u8> = match data.ints(attribute::WORD_STARTS) {
        [] => word_starts_in(text),
        starts => starts
            .iter()
            .filter_map(|unit| unit_characters.get(usize::try_from(*unit).ok()?))
            .filter_map(|character| u8::try_from(*character).ok())
            .collect(),
    };
    word_starts.dedup();
    target.set_word_starts(word_starts);
    target.set_character_lengths(lengths);
    target.set_value(text);
    if let Some(direction) = text_direction(data) {
        target.set_text_direction(direction);
    }
}

/// Where words start, for runs whose words Chromium leaves to the reader to
/// find: at each letter or digit that follows something else.
fn word_starts_in(text: &str) -> Vec<u8> {
    let mut previous = None;
    text.chars()
        .enumerate()
        .filter_map(|(index, character)| {
            let starts = character.is_alphanumeric()
                && previous.is_none_or(|previous: char| !previous.is_alphanumeric());
            previous = Some(character);
            starts.then_some(index)
        })
        .map_while(|index| u8::try_from(index).ok())
        .collect()
}

pub(super) fn text_direction(data: &AxNode) -> Option<TextDirection> {
    Some(match data.string(attribute::TEXT_DIRECTION)? {
        "ltr" => TextDirection::LeftToRight,
        "rtl" => TextDirection::RightToLeft,
        "ttb" => TextDirection::TopToBottom,
        "btt" => TextDirection::BottomToTop,
        _ => return None,
    })
}

pub(super) fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

impl FrameTree {
    /// The caret and selection the focused node holds, when it edits text.
    pub fn text_selection(&self, id: i32) -> Option<TextSelection> {
        if self.data.focus != id {
            return None;
        }
        let node = &self.nodes.get(&id)?.data;
        if let (Some(start), Some(end)) = (
            node.int(attribute::SELECTION_START),
            node.int(attribute::SELECTION_END),
        ) {
            let runs = self.text_runs(id);
            return Some(TextSelection {
                anchor: self.run_position(&runs, usize::try_from(start).ok()?)?,
                focus: self.run_position(&runs, usize::try_from(end).ok()?)?,
            });
        }
        if !node.has_state(state::RICHLY_EDITABLE) {
            return None;
        }
        let (object, offset) = self.data.selection_anchor;
        let (anchor_focus, focus_offset) = self.data.selection_focus;
        let anchor = self.position(object, usize::try_from(offset).ok()?)?;
        let focus = self.position(anchor_focus, usize::try_from(focus_offset).ok()?)?;
        Some(TextSelection { anchor, focus })
    }

    /// Chromium's position of `offset` in `object`: a text offset in text,
    /// and before the child at that index in an element.
    fn position(&self, object: i32, offset: usize) -> Option<TextPosition> {
        let node = &self.nodes.get(&object)?.data;
        if node.role == "staticText" || node.role == "inlineTextBox" {
            return self.run_position(&self.text_runs(object), offset);
        }
        let children: Vec<i32> = self.children_of(object).collect();
        match children.get(offset) {
            Some(child) => self.text_runs(*child).first().map(|run| TextPosition {
                node: node_id(self.index, *run),
                character_index: 0,
            }),
            None => self.run_position(&self.text_runs(object), usize::MAX),
        }
    }

    /// A text run's text. Chromium leaves it out when it is all of the text
    /// above it.
    pub fn run_text(&self, id: i32) -> &str {
        let node = &self.nodes[&id];
        node.data
            .string(attribute::NAME)
            .or_else(|| {
                let parent = self.nodes.get(&node.parent?)?;
                parent.data.string(attribute::NAME)
            })
            .unwrap_or_default()
    }

    /// The text runs at and below `id`, in reading order.
    pub fn text_runs(&self, id: i32) -> Vec<i32> {
        let mut runs = Vec::new();
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            let Some(node) = self.nodes.get(&id) else {
                continue;
            };
            if super::roles::role(&node.data) == Role::TextRun {
                runs.push(id);
            } else {
                let start = pending.len();
                pending.extend(self.children_of(id));
                pending[start..].reverse();
            }
        }
        runs
    }

    /// Where `offset` UTF-16 code units into the runs' text falls, at the end
    /// of the last run when the text is shorter.
    fn run_position(&self, runs: &[i32], offset: usize) -> Option<TextPosition> {
        let mut start = 0;
        let mut last = None;
        for run in runs {
            let text = self.run_text(*run);
            let length = utf16_len(text);
            if offset < start + length {
                let mut units = 0;
                let character_index = text
                    .chars()
                    .take_while(|character| {
                        units += character.len_utf16();
                        units <= offset - start
                    })
                    .count();
                return Some(TextPosition {
                    node: node_id(self.index, *run),
                    character_index,
                });
            }
            start += length;
            last = Some((*run, text.chars().count()));
        }
        last.map(|(run, characters)| TextPosition {
            node: node_id(self.index, run),
            character_index: characters,
        })
    }
}
