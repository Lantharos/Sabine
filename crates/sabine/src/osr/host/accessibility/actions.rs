//! What assistive technology asks of a page node, as the browser host's
//! `accessibility_action` control, which reaches the node's DOM element.

use accesskit::{Action, ActionData, ActionRequest, ScrollUnit, TextPosition};

use super::frame::FrameTree;
use super::page::PageTree;
use super::split_node_id;
use super::text::utf16_len;
use crate::osr::protocol::accessibility::attribute;
use crate::osr::protocol::encode_component;

pub(super) fn control(page: &PageTree, request: &ActionRequest) -> Option<String> {
    let (index, id) = split_node_id(request.target_node);
    let frame = page.frame(index)?;
    let element = dom_node(frame, id)?;
    let parts = match (request.action, &request.data) {
        (Action::Focus, _) => format!("focus\t{element}"),
        (Action::Click | Action::Expand | Action::Collapse, _) => format!("click\t{element}"),
        (Action::ScrollIntoView, _) => format!("scroll_into_view\t{element}"),
        (Action::SetValue, Some(ActionData::Value(value))) => {
            format!("set_value\t{element}\t{}", encode_component(value))
        }
        (Action::SetValue, Some(ActionData::NumericValue(value))) => {
            format!("set_value\t{element}\t{value}")
        }
        (Action::Increment, _) => format!("step\t{element}\t1"),
        (Action::Decrement, _) => format!("step\t{element}\t0"),
        (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
            select(frame, id, element, selection.anchor, selection.focus)?
        }
        (
            Action::ScrollUp | Action::ScrollDown | Action::ScrollLeft | Action::ScrollRight,
            data,
        ) => {
            let (x, y) = match request.action {
                Action::ScrollUp => (0, -1),
                Action::ScrollDown => (0, 1),
                Action::ScrollLeft => (-1, 0),
                _ => (1, 0),
            };
            let page = !matches!(data, Some(ActionData::ScrollUnit(ScrollUnit::Item)));
            format!("scroll\t{element}\t{x}\t{y}\t{}", u8::from(page))
        }
        _ => return None,
    };
    Some(format!("accessibility_action\t{parts}\n"))
}

/// The DOM node an accessibility node stands for: Chromium names the nodes
/// it makes up itself, such as text boxes, below zero, and those act through
/// the nearest node above them that has one.
fn dom_node(frame: &FrameTree, mut id: i32) -> Option<i32> {
    while id <= 0 {
        id = frame.nodes.get(&id)?.parent?;
    }
    Some(id)
}

/// A text field selects by offsets into its value, and other text by DOM
/// text nodes and offsets into them.
fn select(
    frame: &FrameTree,
    target: i32,
    element: i32,
    anchor: TextPosition,
    focus: TextPosition,
) -> Option<String> {
    let field = &frame.nodes.get(&target)?.data;
    if field.int(attribute::SELECTION_START).is_some() {
        let runs = frame.text_runs(target);
        let start = offset_in(frame, &runs, anchor)?;
        let end = offset_in(frame, &runs, focus)?;
        return Some(format!("select_text\t{element}\t{start}\t{end}"));
    }
    let (anchor_text, anchor_offset) = text_offset(frame, anchor)?;
    let (focus_text, focus_offset) = text_offset(frame, focus)?;
    Some(format!(
        "select_range\t{anchor_text}\t{anchor_offset}\t{focus_text}\t{focus_offset}"
    ))
}

/// The DOM text node a position is in, and the UTF-16 offset into it.
fn text_offset(frame: &FrameTree, position: TextPosition) -> Option<(i32, usize)> {
    let (_, run) = split_node_id(position.node);
    let text = dom_node(frame, frame.nodes.get(&run)?.parent?)?;
    Some((text, offset_in(frame, &frame.text_runs(text), position)?))
}

/// How many UTF-16 code units of the runs' text come before the position.
fn offset_in(frame: &FrameTree, runs: &[i32], position: TextPosition) -> Option<usize> {
    let (_, target) = split_node_id(position.node);
    let mut offset = 0;
    for run in runs {
        let text = frame.run_text(*run);
        if *run == target {
            let before: usize = text
                .chars()
                .take(position.character_index)
                .map(char::len_utf16)
                .sum();
            return Some(offset + before);
        }
        offset += utf16_len(text);
    }
    None
}
