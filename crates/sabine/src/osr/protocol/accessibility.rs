//! The page's accessibility trees as the browser host forwards them: the
//! updates Chromium makes to each frame's tree, cut down to what the window
//! exposes, matching `osr/accessibility/tree_wire.cc`.

/// Updates to one frame's tree, in the order Chromium made them.
#[derive(Debug)]
pub(crate) struct AxTreeChange {
    pub tree: String,
    pub updates: Vec<AxTreeUpdate>,
}

#[derive(Debug)]
pub(crate) struct AxTreeUpdate {
    pub data: Option<AxTreeData>,
    /// The node whose descendants the update replaces, or 0.
    pub clear: i32,
    /// The tree's new root, or 0 when it keeps its root.
    pub root: i32,
    pub nodes: Vec<AxNode>,
}

/// What Chromium tells about a frame's tree as a whole.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct AxTreeData {
    pub parent_tree: String,
    pub focused_tree: String,
    pub focus: i32,
    pub selection_anchor: (i32, i32),
    pub selection_focus: (i32, i32),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AxNode {
    pub id: i32,
    pub role: String,
    pub states: u32,
    pub actions: u32,
    pub location: AxLocation,
    pub children: Vec<i32>,
    pub attributes: Vec<(u8, AxValue)>,
}

impl AxTreeChange {
    /// The bytes the change keeps alive while it waits for the window.
    pub fn retained_bytes(&self) -> usize {
        self.updates
            .iter()
            .flat_map(|update| &update.nodes)
            .map(AxNode::retained_bytes)
            .sum()
    }
}

impl AxNode {
    fn retained_bytes(&self) -> usize {
        let attributes: usize = self
            .attributes
            .iter()
            .map(|(_, value)| match value {
                AxValue::String(text) => text.capacity(),
                AxValue::Ints(values) => values.capacity() * size_of::<i32>(),
                _ => 0,
            })
            .sum();
        size_of::<Self>()
            + self.role.capacity()
            + self.children.capacity() * size_of::<i32>()
            + self.attributes.capacity() * size_of::<(u8, AxValue)>()
            + attributes
    }

    pub fn has_state(&self, state: u32) -> bool {
        self.states & (1 << state) != 0
    }

    pub fn has_action(&self, action: u32) -> bool {
        self.actions & (1 << action) != 0
    }

    fn attribute(&self, code: u8) -> Option<&AxValue> {
        self.attributes
            .iter()
            .find_map(|(key, value)| (*key == code).then_some(value))
    }

    pub fn string(&self, code: u8) -> Option<&str> {
        match self.attribute(code)? {
            AxValue::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn int(&self, code: u8) -> Option<i32> {
        match self.attribute(code)? {
            AxValue::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub fn float(&self, code: u8) -> Option<f32> {
        match self.attribute(code)? {
            AxValue::Float(value) => Some(*value),
            _ => None,
        }
    }

    pub fn boolean(&self, code: u8) -> Option<bool> {
        match self.attribute(code)? {
            AxValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn flag(&self, code: u8) -> bool {
        self.boolean(code) == Some(true)
    }

    pub fn ints(&self, code: u8) -> &[i32] {
        match self.attribute(code) {
            Some(AxValue::Ints(values)) => values,
            _ => &[],
        }
    }
}

/// Where a node is: its box relative to its offset container, before the
/// node's own transform, in the page's pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct AxLocation {
    /// The node whose box the location is relative to, or -1 for the root.
    pub container: i32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// A 2D transform, as `a, b, c, d, e, f` of `x' = a x + c y + e`,
    /// `y' = b x + d y + f`.
    pub transform: Option<[f32; 6]>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AxValue {
    String(String),
    Int(i32),
    Float(f32),
    Bool(bool),
    Ints(Vec<i32>),
}

/// Nodes of one frame's tree that moved.
#[derive(Debug)]
pub(crate) struct AxLocationChange {
    pub tree: String,
    pub locations: Vec<(i32, AxLocation)>,
}

pub(crate) mod state {
    pub const COLLAPSED: u32 = 0;
    pub const DEFAULT: u32 = 1;
    pub const EDITABLE: u32 = 2;
    pub const EXPANDED: u32 = 3;
    pub const FOCUSABLE: u32 = 4;
    pub const HORIZONTAL: u32 = 5;
    pub const IGNORED: u32 = 7;
    pub const INVISIBLE: u32 = 8;
    pub const MULTILINE: u32 = 10;
    pub const MULTISELECTABLE: u32 = 11;
    pub const PROTECTED: u32 = 12;
    pub const REQUIRED: u32 = 13;
    pub const RICHLY_EDITABLE: u32 = 14;
    pub const VERTICAL: u32 = 15;
    pub const VISITED: u32 = 16;
}

pub(crate) mod action {
    pub const DECREMENT: u32 = 1;
    pub const INCREMENT: u32 = 5;
    pub const SCROLL_UP: u32 = 8;
    pub const SCROLL_DOWN: u32 = 9;
    pub const SCROLL_LEFT: u32 = 10;
    pub const SCROLL_RIGHT: u32 = 11;
    pub const SET_VALUE: u32 = 12;
}

pub(crate) mod attribute {
    pub const NAME: u8 = 1;
    pub const VALUE: u8 = 2;
    pub const DESCRIPTION: u8 = 3;
    pub const PLACEHOLDER: u8 = 4;
    pub const URL: u8 = 5;
    pub const CHILD_TREE: u8 = 6;
    pub const LIVE: u8 = 7;
    pub const ROLE_DESCRIPTION: u8 = 9;
    pub const KEY_SHORTCUTS: u8 = 10;
    pub const ACCESS_KEY: u8 = 11;
    pub const TOOLTIP: u8 = 12;
    pub const LANGUAGE: u8 = 13;
    pub const CHECKED: u8 = 14;
    pub const INVALID: u8 = 15;
    pub const RESTRICTION: u8 = 16;
    pub const DEFAULT_ACTION: u8 = 17;
    pub const CURRENT: u8 = 18;
    pub const SORT: u8 = 19;
    pub const TEXT_DIRECTION: u8 = 20;
    pub const INPUT_TYPE: u8 = 21;
    pub const VALUE_TEXT: u8 = 22;
    pub const SCROLL_X: u8 = 30;
    pub const SCROLL_X_MIN: u8 = 31;
    pub const SCROLL_X_MAX: u8 = 32;
    pub const SCROLL_Y: u8 = 33;
    pub const SCROLL_Y_MIN: u8 = 34;
    pub const SCROLL_Y_MAX: u8 = 35;
    pub const SELECTION_START: u8 = 36;
    pub const SELECTION_END: u8 = 37;
    pub const LEVEL: u8 = 38;
    pub const SET_SIZE: u8 = 39;
    pub const POSITION_IN_SET: u8 = 40;
    pub const ACTIVE_DESCENDANT: u8 = 41;
    pub const ROW_COUNT: u8 = 42;
    pub const COLUMN_COUNT: u8 = 43;
    pub const ROW_INDEX: u8 = 44;
    pub const COLUMN_INDEX: u8 = 45;
    pub const CELL_ROW_INDEX: u8 = 46;
    pub const CELL_COLUMN_INDEX: u8 = 47;
    pub const ROW_SPAN: u8 = 48;
    pub const COLUMN_SPAN: u8 = 49;
    pub const HAS_POPUP: u8 = 50;
    pub const RANGE_VALUE: u8 = 60;
    pub const RANGE_MIN: u8 = 61;
    pub const RANGE_MAX: u8 = 62;
    pub const RANGE_STEP: u8 = 63;
    pub const BUSY: u8 = 70;
    pub const LIVE_ATOMIC: u8 = 71;
    pub const MODAL: u8 = 72;
    pub const SELECTED: u8 = 73;
    pub const SCROLLABLE: u8 = 74;
    pub const TEXT_FIELD_ROOT: u8 = 75;
    pub const CHARACTER_OFFSETS: u8 = 80;
    pub const WORD_STARTS: u8 = 81;
    pub const LABELLED_BY: u8 = 83;
    pub const DESCRIBED_BY: u8 = 84;
    pub const CONTROLS: u8 = 85;
    pub const DETAILS: u8 = 86;
    pub const FLOW_TO: u8 = 87;
    pub const ERROR_MESSAGE: u8 = 88;
}
