use std::collections::{HashMap, HashSet};

use accesskit::{Affine, Rect};

use crate::osr::protocol::accessibility::{
    AxLocation, AxNode, AxTreeData, AxTreeUpdate, attribute,
};

/// One frame's accessibility tree as Chromium last described it.
pub(super) struct FrameTree {
    pub index: u32,
    pub root: Option<i32>,
    pub nodes: HashMap<i32, PageNode>,
    pub data: AxTreeData,
}

pub(super) struct PageNode {
    pub data: AxNode,
    pub parent: Option<i32>,
    /// Maps the node's content, which its offset children are placed in, to
    /// the frame's document.
    pub content: Affine,
    /// The node's box in the frame's document, before the document scrolls.
    pub bounds: Option<Rect>,
}

/// What applying Chromium's updates changed in a frame.
#[derive(Default)]
pub(super) struct FrameChanges {
    pub nodes: HashSet<i32>,
    /// Frames whose iframe went away.
    pub dropped_frames: Vec<String>,
    /// Frames an iframe now shows.
    pub hosted_frames: Vec<(String, i32)>,
    pub root_changed: bool,
}

impl FrameTree {
    pub fn new(index: u32) -> Self {
        Self {
            index,
            root: None,
            nodes: HashMap::new(),
            data: AxTreeData::default(),
        }
    }

    pub fn apply(&mut self, update: AxTreeUpdate, changes: &mut FrameChanges) {
        if let Some(data) = update.data {
            changes.nodes.extend([self.data.focus, data.focus]);
            self.data = data;
        }
        let mut detached = Vec::new();
        if update.clear != 0
            && let Some(node) = self.nodes.get_mut(&update.clear)
        {
            detached.append(&mut node.data.children);
            changes.nodes.insert(update.clear);
        }
        if update.root != 0 && self.root != Some(update.root) {
            detached.extend(self.root);
            self.root = Some(update.root);
            changes.root_changed = true;
        }
        let updated: Vec<i32> = update.nodes.iter().map(|node| node.id).collect();
        for node in update.nodes {
            self.store(node, &mut detached, changes);
        }
        for id in &updated {
            let children = self.nodes[id].data.children.clone();
            for child in children {
                if let Some(child) = self.nodes.get_mut(&child) {
                    child.parent = Some(*id);
                }
            }
        }
        if let Some(root) = self.root.and_then(|root| self.nodes.get_mut(&root)) {
            root.parent = None;
        }
        for id in detached {
            if self.is_orphan(id) {
                self.remove(id, changes);
            }
        }
        self.relayout(updated, &mut changes.nodes);
    }

    fn store(&mut self, node: AxNode, detached: &mut Vec<i32>, changes: &mut FrameChanges) {
        let id = node.id;
        changes.nodes.insert(id);
        let hosted = node.string(attribute::CHILD_TREE).map(str::to_string);
        match self.nodes.get_mut(&id) {
            Some(existing) => {
                detached.extend(
                    existing
                        .data
                        .children
                        .iter()
                        .filter(|child| !node.children.contains(child)),
                );
                let previous = existing.data.string(attribute::CHILD_TREE);
                if let Some(previous) = previous
                    && Some(previous) != hosted.as_deref()
                {
                    changes.dropped_frames.push(previous.to_string());
                }
                existing.data = node;
            }
            None => {
                self.nodes.insert(
                    id,
                    PageNode {
                        data: node,
                        parent: None,
                        content: Affine::IDENTITY,
                        bounds: None,
                    },
                );
            }
        }
        if let Some(hosted) = hosted {
            changes.hosted_frames.push((hosted, id));
        }
    }

    /// Whether no node lists `id` as its child any more.
    fn is_orphan(&self, id: i32) -> bool {
        let Some(node) = self.nodes.get(&id) else {
            return false;
        };
        Some(id) != self.root
            && node.parent.is_none_or(|parent| {
                self.nodes
                    .get(&parent)
                    .is_none_or(|parent| !parent.data.children.contains(&id))
            })
    }

    fn remove(&mut self, id: i32, changes: &mut FrameChanges) {
        let mut pending = vec![id];
        while let Some(id) = pending.pop() {
            let Some(node) = self.nodes.remove(&id) else {
                continue;
            };
            changes.nodes.remove(&id);
            if let Some(hosted) = node.data.string(attribute::CHILD_TREE) {
                changes.dropped_frames.push(hosted.to_string());
            }
            pending.extend(node.data.children.iter().copied().filter(|child| {
                self.nodes
                    .get(child)
                    .is_some_and(|child| child.parent == Some(id))
            }));
        }
    }

    pub fn move_nodes(&mut self, locations: Vec<(i32, AxLocation)>, changed: &mut HashSet<i32>) {
        let mut moved = Vec::with_capacity(locations.len());
        for (id, location) in locations {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.data.location = location;
                moved.push(id);
            }
        }
        self.relayout(moved, changed);
    }

    /// The node's children that are still in the tree and belong to it.
    pub fn children_of(&self, id: i32) -> impl Iterator<Item = i32> + '_ {
        self.nodes
            .get(&id)
            .into_iter()
            .flat_map(|node| node.data.children.iter().copied())
            .filter(move |child| {
                self.nodes
                    .get(child)
                    .is_some_and(|child| child.parent == Some(id))
            })
    }

    /// Every node hanging from the frame's root.
    pub fn subtree(&self) -> impl Iterator<Item = i32> + '_ {
        let mut pending: Vec<i32> = self.root.into_iter().collect();
        std::iter::from_fn(move || {
            let id = pending.pop()?;
            pending.extend(self.children_of(id));
            Some(id)
        })
    }

    /// Whether the node hangs from the frame's root.
    pub fn reaches_root(&self, mut id: i32) -> bool {
        loop {
            if Some(id) == self.root {
                return self.nodes.contains_key(&id);
            }
            match self.nodes.get(&id).and_then(|node| node.parent) {
                Some(parent) => id = parent,
                None => return false,
            }
        }
    }
}
