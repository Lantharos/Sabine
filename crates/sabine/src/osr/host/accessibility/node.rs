//! A page node as assistive technology sees it.

use accesskit::{
    Action, Affine, AriaCurrent, HasPopup, Invalid, Live, Node, NodeId, Orientation, Rect, Role,
    SortDirection, Toggled, Vec2,
};

use super::frame::{FrameTree, PageNode};
use super::{node_id, roles, text};
use crate::osr::protocol::accessibility::{AxNode, action, attribute, state};

/// Sets one of a node's properties from a page attribute.
type Setter<T> = fn(&mut Node, T);

/// Builds the node `id` of `frame`, whose children are `children`, for a
/// frame whose document starts at `origin` in the window.
pub(super) fn build(frame: &FrameTree, id: i32, children: Vec<NodeId>, origin: Vec2) -> Node {
    let page_node = &frame.nodes[&id];
    let data = &page_node.data;
    let role = roles::role(data);
    let mut node = Node::new(role);
    node.set_children(children);
    place(&mut node, frame, id, page_node, origin);
    if role == Role::TextRun {
        text::set_text_run(&mut node, data, frame.run_text(id));
        return node;
    }
    describe(&mut node, data, role);
    relate(&mut node, frame, data);
    set_states(&mut node, data);
    set_values(&mut node, data);
    add_actions(&mut node, data, role);
    if let Some(selection) = frame.text_selection(id) {
        node.set_text_selection(selection);
    }
    node
}

/// The root places and scrolls the document beneath it, and an iframe shows
/// its frame's document from its own corner.
fn place(node: &mut Node, frame: &FrameTree, id: i32, page_node: &PageNode, origin: Vec2) {
    if Some(id) == frame.root {
        let scroll = frame.document_scroll();
        node.set_transform(Affine::translate(origin - scroll));
        if let Some(bounds) = page_node.bounds {
            node.set_bounds(bounds + scroll);
        }
        return;
    }
    let Some(bounds) = page_node.bounds else {
        return;
    };
    if page_node.data.string(attribute::CHILD_TREE).is_some() {
        node.set_transform(Affine::translate(bounds.origin().to_vec2()));
        node.set_bounds(Rect::from_origin_size((0.0, 0.0), bounds.size()));
    } else {
        node.set_bounds(bounds);
    }
}

/// Static text reads as its value, which other nodes have besides their name.
fn describe(node: &mut Node, data: &AxNode, role: Role) {
    let name = data.string(attribute::NAME).filter(|name| !name.is_empty());
    if role == Role::Label {
        if let Some(name) = name {
            node.set_value(name);
        }
    } else {
        if let Some(name) = name {
            node.set_label(name);
        }
        if let Some(value) = data
            .string(attribute::VALUE_TEXT)
            .filter(|value| !value.is_empty())
            .or(data.string(attribute::VALUE))
        {
            node.set_value(value);
        }
    }
    let strings: [(u8, Setter<String>); 8] = [
        (attribute::DESCRIPTION, |node, value| {
            node.set_description(value)
        }),
        (attribute::PLACEHOLDER, |node, value| {
            node.set_placeholder(value)
        }),
        (attribute::URL, |node, value| node.set_url(value)),
        (attribute::ROLE_DESCRIPTION, |node, value| {
            node.set_role_description(value)
        }),
        (attribute::KEY_SHORTCUTS, |node, value| {
            node.set_keyboard_shortcut(value)
        }),
        (attribute::ACCESS_KEY, |node, value| {
            node.set_access_key(value)
        }),
        (attribute::TOOLTIP, |node, value| node.set_tooltip(value)),
        (attribute::LANGUAGE, |node, value| node.set_language(value)),
    ];
    for (code, set) in strings {
        if let Some(value) = data.string(code).filter(|value| !value.is_empty()) {
            set(node, value.to_string());
        }
    }
    if let Some(direction) = text::text_direction(data) {
        node.set_text_direction(direction);
    }
}

fn relate(node: &mut Node, frame: &FrameTree, data: &AxNode) {
    let ids = |code| {
        data.ints(code)
            .iter()
            .filter(|id| frame.nodes.contains_key(id))
            .map(|id| node_id(frame.index, *id))
            .collect::<Vec<_>>()
    };
    node.set_labelled_by(ids(attribute::LABELLED_BY));
    node.set_described_by(ids(attribute::DESCRIBED_BY));
    node.set_controls(ids(attribute::CONTROLS));
    node.set_details(ids(attribute::DETAILS));
    node.set_flow_to(ids(attribute::FLOW_TO));
    if let Some(message) = ids(attribute::ERROR_MESSAGE).first() {
        node.set_error_message(*message);
    }
    if let Some(descendant) = data
        .int(attribute::ACTIVE_DESCENDANT)
        .filter(|id| frame.nodes.contains_key(id))
    {
        node.set_active_descendant(node_id(frame.index, descendant));
    }
}

fn set_states(node: &mut Node, data: &AxNode) {
    if data.has_state(state::EXPANDED) {
        node.set_expanded(true);
    } else if data.has_state(state::COLLAPSED) {
        node.set_expanded(false);
    }
    if data.has_state(state::HORIZONTAL) {
        node.set_orientation(Orientation::Horizontal);
    } else if data.has_state(state::VERTICAL) {
        node.set_orientation(Orientation::Vertical);
    }
    if data.has_state(state::MULTISELECTABLE) {
        node.set_multiselectable();
    }
    if data.has_state(state::REQUIRED) {
        node.set_required();
    }
    if data.has_state(state::VISITED) {
        node.set_visited();
    }
    if data.has_state(state::DEFAULT) && node.role() == Role::Button {
        node.set_role(Role::DefaultButton);
    }
    match data.string(attribute::RESTRICTION) {
        Some("disabled") => node.set_disabled(),
        Some("readOnly") => node.set_read_only(),
        _ => {}
    }
    match data.string(attribute::CHECKED) {
        Some("true") => node.set_toggled(Toggled::True),
        Some("false") => node.set_toggled(Toggled::False),
        Some("mixed") => node.set_toggled(Toggled::Mixed),
        _ => {}
    }
    if data.string(attribute::INVALID) == Some("true") {
        node.set_invalid(Invalid::True);
    }
    if let Some(live) = live(data.string(attribute::LIVE)) {
        node.set_live(live);
    }
    if data.flag(attribute::LIVE_ATOMIC) {
        node.set_live_atomic();
    }
    if data.flag(attribute::BUSY) {
        node.set_busy();
    }
    if data.flag(attribute::MODAL) {
        node.set_modal();
    }
    if let Some(selected) = data.boolean(attribute::SELECTED) {
        node.set_selected(selected);
    }
    if let Some(current) = current(data.string(attribute::CURRENT)) {
        node.set_aria_current(current);
    }
    if let Some(popup) = data.int(attribute::HAS_POPUP).and_then(popup) {
        node.set_has_popup(popup);
    }
    match data.string(attribute::SORT) {
        Some("ascending") => node.set_sort_direction(SortDirection::Ascending),
        Some("descending") => node.set_sort_direction(SortDirection::Descending),
        Some("other") => node.set_sort_direction(SortDirection::Other),
        _ => {}
    }
}

fn set_values(node: &mut Node, data: &AxNode) {
    let floats: [(u8, Setter<f64>); 10] = [
        (attribute::RANGE_VALUE, Node::set_numeric_value),
        (attribute::RANGE_MIN, Node::set_min_numeric_value),
        (attribute::RANGE_MAX, Node::set_max_numeric_value),
        (attribute::RANGE_STEP, Node::set_numeric_value_step),
        (attribute::SCROLL_X, Node::set_scroll_x),
        (attribute::SCROLL_X_MIN, Node::set_scroll_x_min),
        (attribute::SCROLL_X_MAX, Node::set_scroll_x_max),
        (attribute::SCROLL_Y, Node::set_scroll_y),
        (attribute::SCROLL_Y_MIN, Node::set_scroll_y_min),
        (attribute::SCROLL_Y_MAX, Node::set_scroll_y_max),
    ];
    for (code, set) in floats {
        let value = data
            .float(code)
            .map(f64::from)
            .or_else(|| data.int(code).map(f64::from));
        if let Some(value) = value {
            set(node, value);
        }
    }
    let counts: [(u8, Setter<usize>); 11] = [
        (attribute::LEVEL, Node::set_level),
        (attribute::SET_SIZE, Node::set_size_of_set),
        (attribute::POSITION_IN_SET, Node::set_position_in_set),
        (attribute::ROW_COUNT, Node::set_row_count),
        (attribute::COLUMN_COUNT, Node::set_column_count),
        (attribute::ROW_INDEX, Node::set_row_index),
        (attribute::COLUMN_INDEX, Node::set_column_index),
        (attribute::CELL_ROW_INDEX, Node::set_row_index),
        (attribute::CELL_COLUMN_INDEX, Node::set_column_index),
        (attribute::ROW_SPAN, Node::set_row_span),
        (attribute::COLUMN_SPAN, Node::set_column_span),
    ];
    for (code, set) in counts {
        if let Some(count) = data.int(code).and_then(|count| usize::try_from(count).ok()) {
            set(node, count);
        }
    }
}

fn add_actions(node: &mut Node, data: &AxNode, role: Role) {
    if data.has_state(state::FOCUSABLE) {
        node.add_action(Action::Focus);
    }
    if data
        .string(attribute::DEFAULT_ACTION)
        .is_some_and(|verb| !matches!(verb, "none" | "clickAncestor"))
    {
        node.add_action(Action::Click);
    }
    if data.id > 0 {
        node.add_action(Action::ScrollIntoView);
    }
    let editable = data.has_state(state::EDITABLE);
    if editable && !data.has_state(state::RICHLY_EDITABLE) || data.has_action(action::SET_VALUE) {
        node.add_action(Action::SetValue);
    }
    if editable {
        node.add_action(Action::SetTextSelection);
    }
    let ranged = matches!(role, Role::Slider | Role::SpinButton);
    if ranged || data.has_action(action::INCREMENT) {
        node.add_action(Action::Increment);
    }
    if ranged || data.has_action(action::DECREMENT) {
        node.add_action(Action::Decrement);
    }
    let scrolls = [
        (action::SCROLL_UP, Action::ScrollUp),
        (action::SCROLL_DOWN, Action::ScrollDown),
        (action::SCROLL_LEFT, Action::ScrollLeft),
        (action::SCROLL_RIGHT, Action::ScrollRight),
    ];
    for (chromium, scroll) in scrolls {
        if data.has_action(chromium) || data.flag(attribute::SCROLLABLE) {
            node.add_action(scroll);
        }
    }
    if data.has_state(state::EXPANDED) {
        node.add_action(Action::Collapse);
    } else if data.has_state(state::COLLAPSED) {
        node.add_action(Action::Expand);
    }
}

fn live(value: Option<&str>) -> Option<Live> {
    Some(match value? {
        "polite" => Live::Polite,
        "assertive" => Live::Assertive,
        "off" => Live::Off,
        _ => return None,
    })
}

fn current(value: Option<&str>) -> Option<AriaCurrent> {
    Some(match value? {
        "true" => AriaCurrent::True,
        "page" => AriaCurrent::Page,
        "step" => AriaCurrent::Step,
        "location" => AriaCurrent::Location,
        "date" => AriaCurrent::Date,
        "time" => AriaCurrent::Time,
        _ => return None,
    })
}

/// Chromium's `ax::mojom::HasPopup`, where ARIA's `true` means a menu.
fn popup(value: i32) -> Option<HasPopup> {
    Some(match value {
        1 | 2 => HasPopup::Menu,
        3 => HasPopup::Listbox,
        4 => HasPopup::Tree,
        5 => HasPopup::Grid,
        6 => HasPopup::Dialog,
        _ => return None,
    })
}
