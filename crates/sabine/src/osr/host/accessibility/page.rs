use std::collections::{HashMap, HashSet};

use accesskit::{Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate, Vec2};

use super::frame::{FrameChanges, FrameTree};
use super::{WINDOW, node, node_id, split_node_id};
use crate::osr::protocol::accessibility::{AxLocationChange, AxTreeChange, attribute};

/// The window as assistive technology sees it: the window itself and the
/// page's frames below it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct WindowGeometry {
    pub title: String,
    pub width: f64,
    pub height: f64,
    /// Where the page starts in the window, in its pixels.
    pub page_origin: Vec2,
}

#[derive(Default)]
pub(super) struct PageTree {
    frames: HashMap<String, FrameTree>,
    trees: HashMap<u32, String>,
    /// The iframe each frame shows in, by the frame's tree.
    hosts: HashMap<String, (u32, i32)>,
    main: Option<String>,
    next_index: u32,
    window: WindowGeometry,
    dirty: HashSet<NodeId>,
    /// The frames whose nodes assistive technology holds.
    shown_frames: HashSet<u32>,
}

impl PageTree {
    pub fn set_window(&mut self, window: WindowGeometry) {
        if self.window == window {
            return;
        }
        if self.window.page_origin != window.page_origin
            && let Some(root) = self.main_root()
        {
            self.dirty.insert(root);
        }
        self.window = window;
        self.dirty.insert(WINDOW);
    }

    pub fn clear(&mut self) {
        let window = std::mem::take(&mut self.window);
        *self = Self {
            window,
            ..Self::default()
        };
        self.dirty.insert(WINDOW);
    }

    pub fn apply_tree(&mut self, change: AxTreeChange) {
        let index = self.frame_index(&change.tree);
        let frame = self.frames.get_mut(&change.tree).expect("frame was added");
        let mut changes = FrameChanges::default();
        for update in change.updates {
            frame.apply(update, &mut changes);
        }
        let candidate = frame.data.parent_tree.is_empty() && frame.root.is_some();
        self.dirty
            .extend(changes.nodes.iter().map(|id| node_id(index, *id)));
        for (tree, host) in changes.hosted_frames {
            self.hosts.insert(tree, (index, host));
        }
        for tree in changes.dropped_frames {
            if self
                .hosts
                .get(&tree)
                .is_some_and(|host| !self.hosts_frame(*host, &tree))
            {
                self.remove_frame(&tree);
            }
        }
        if candidate && self.main.as_ref() != Some(&change.tree) {
            if let Some(previous) = self.main.replace(change.tree.clone()) {
                self.remove_frame(&previous);
            }
            self.dirty.insert(WINDOW);
        }
        if changes.root_changed {
            match self.hosts.get(&change.tree) {
                Some((host_index, host)) => {
                    self.dirty.insert(node_id(*host_index, *host));
                }
                None => {
                    self.dirty.insert(WINDOW);
                }
            }
        }
    }

    pub fn move_nodes(&mut self, change: AxLocationChange) {
        let Some(frame) = self.frames.get_mut(&change.tree) else {
            return;
        };
        let mut moved = HashSet::new();
        frame.move_nodes(change.locations, &mut moved);
        let index = frame.index;
        self.dirty
            .extend(moved.into_iter().map(|id| node_id(index, id)));
    }

    fn frame_index(&mut self, tree: &str) -> u32 {
        if let Some(frame) = self.frames.get(tree) {
            return frame.index;
        }
        self.next_index += 1;
        let index = self.next_index;
        self.frames.insert(tree.to_string(), FrameTree::new(index));
        self.trees.insert(index, tree.to_string());
        index
    }

    fn remove_frame(&mut self, tree: &str) {
        let Some(frame) = self.frames.remove(tree) else {
            return;
        };
        self.trees.remove(&frame.index);
        if let Some((host_index, host)) = self.hosts.remove(tree) {
            self.dirty.insert(node_id(host_index, host));
        }
        let nested: Vec<String> = self
            .hosts
            .iter()
            .filter(|(_, (host_index, _))| *host_index == frame.index)
            .map(|(tree, _)| tree.clone())
            .collect();
        for tree in nested {
            self.remove_frame(&tree);
        }
    }

    /// Whether the iframe node `host` shows the frame `tree`.
    fn hosts_frame(&self, (index, host): (u32, i32), tree: &str) -> bool {
        self.frame(index)
            .and_then(|frame| frame.nodes.get(&host))
            .is_some_and(|node| node.data.string(attribute::CHILD_TREE) == Some(tree))
    }

    pub(super) fn frame(&self, index: u32) -> Option<&FrameTree> {
        self.frames.get(self.trees.get(&index)?)
    }

    fn main_frame(&self) -> Option<&FrameTree> {
        self.frames.get(self.main.as_ref()?)
    }

    fn main_root(&self) -> Option<NodeId> {
        let frame = self.main_frame()?;
        Some(node_id(frame.index, frame.root?))
    }

    /// The root of the frame the iframe node `id` shows.
    fn hosted_root(&self, frame: &FrameTree, id: i32) -> Option<NodeId> {
        let tree = frame.nodes[&id].data.string(attribute::CHILD_TREE)?;
        let hosted = self.frames.get(tree)?;
        (self.hosts.get(tree) == Some(&(frame.index, id)))
            .then(|| Some(node_id(hosted.index, hosted.root?)))
            .flatten()
    }

    /// Whether the node is in the tree the window shows.
    fn is_shown(&self, frame: &FrameTree, id: i32) -> bool {
        if !frame.reaches_root(id) {
            return false;
        }
        let tree = &self.trees[&frame.index];
        if self.main.as_ref() == Some(tree) {
            return true;
        }
        let Some(&(host_index, host)) = self.hosts.get(tree) else {
            return false;
        };
        self.frame(host_index).is_some_and(|host_frame| {
            host_frame.nodes.contains_key(&host)
                && self.hosts_frame((host_index, host), tree)
                && self.is_shown(host_frame, host)
        })
    }

    fn children(&self, frame: &FrameTree, id: i32) -> Vec<NodeId> {
        match self.hosted_root(frame, id) {
            Some(root) => vec![root],
            None if frame.nodes[&id]
                .data
                .string(attribute::CHILD_TREE)
                .is_some() =>
            {
                Vec::new()
            }
            None => frame
                .children_of(id)
                .map(|child| node_id(frame.index, child))
                .collect(),
        }
    }

    fn build(&self, frame: &FrameTree, id: i32) -> Node {
        let main = self.main.as_ref() == self.trees.get(&frame.index);
        let origin = if main {
            self.window.page_origin
        } else {
            Vec2::ZERO
        };
        node::build(frame, id, self.children(frame, id), origin)
    }

    fn window_node(&self) -> Node {
        let mut window = Node::new(Role::Window);
        window.set_label(self.window.title.clone());
        window.set_bounds(Rect::new(0.0, 0.0, self.window.width, self.window.height));
        window.set_children(self.main_root().into_iter().collect::<Vec<_>>());
        window
    }

    fn focus(&self) -> NodeId {
        let Some(main) = self.main_frame() else {
            return WINDOW;
        };
        let focused = self
            .frames
            .get(&main.data.focused_tree)
            .filter(|frame| self.is_shown(frame, frame.data.focus));
        let frame = focused.unwrap_or(main);
        if frame.nodes.contains_key(&frame.data.focus) && self.is_shown(frame, frame.data.focus) {
            node_id(frame.index, frame.data.focus)
        } else {
            WINDOW
        }
    }

    fn update(&self, nodes: Vec<(NodeId, Node)>) -> TreeUpdate {
        TreeUpdate {
            nodes,
            tree: Some(TreeInfo::new(WINDOW)),
            tree_id: TreeId::ROOT,
            focus: self.focus(),
        }
    }

    /// The nodes that changed since the last update, for a tree assistive
    /// technology already holds, with every node of the frames it now
    /// reaches for the first time.
    pub fn take_update(&mut self) -> TreeUpdate {
        let mut ids = std::mem::take(&mut self.dirty);
        let shown: HashSet<u32> = self
            .frames
            .values()
            .filter(|frame| frame.root.is_some_and(|root| self.is_shown(frame, root)))
            .map(|frame| frame.index)
            .collect();
        for index in shown.difference(&self.shown_frames) {
            if let Some(frame) = self.frame(*index) {
                ids.extend(frame.subtree().map(|id| node_id(*index, id)));
            }
        }
        self.shown_frames = shown;
        let nodes = ids
            .into_iter()
            .filter_map(|id| {
                if id == WINDOW {
                    return Some((WINDOW, self.window_node()));
                }
                let (index, node) = split_node_id(id);
                let frame = self.frame(index)?;
                (self.shown_frames.contains(&index) && frame.reaches_root(node))
                    .then(|| (id, self.build(frame, node)))
            })
            .collect();
        self.update(nodes)
    }

    /// The whole tree, for assistive technology that starts reading it.
    pub fn full_update(&mut self) -> TreeUpdate {
        let mut nodes = vec![(WINDOW, self.window_node())];
        let mut shown = HashSet::new();
        let mut pending: Vec<NodeId> = self.main_root().into_iter().collect();
        while let Some(id) = pending.pop() {
            let (index, node) = split_node_id(id);
            let Some(frame) = self.frame(index) else {
                continue;
            };
            shown.insert(index);
            let built = self.build(frame, node);
            pending.extend(built.children().iter().copied());
            nodes.push((id, built));
        }
        self.shown_frames = shown;
        self.update(nodes)
    }
}
