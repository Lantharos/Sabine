//! Where page nodes are. Chromium places each node relative to its offset
//! container, scrolled and transformed along the way, so a node's place in
//! the document follows from its containers' (see `AXTree::RelativeToTreeBounds`).
//! The document's own scroll stays out, as the transform of its root node, so
//! scrolling the page moves no other node.

use std::collections::HashSet;

use accesskit::{Affine, Rect, Vec2};

use super::frame::FrameTree;
use crate::osr::protocol::accessibility::{AxLocation, attribute};

impl FrameTree {
    /// Places the moved nodes and every node placed relative to them anew,
    /// adding those whose box changed to `changed`.
    pub fn relayout(&mut self, mut moved: Vec<i32>, changed: &mut HashSet<i32>) {
        moved.retain(|id| self.nodes.contains_key(id));
        moved.sort_by_cached_key(|id| self.depth(*id));
        let mut placed = HashSet::new();
        for id in moved {
            if !placed.contains(&id) {
                self.place_subtree(id, &mut placed, changed);
            }
        }
    }

    fn depth(&self, mut id: i32) -> usize {
        let mut depth = 0;
        while let Some(parent) = self.nodes.get(&id).and_then(|node| node.parent) {
            depth += 1;
            id = parent;
        }
        depth
    }

    fn place_subtree(&mut self, id: i32, placed: &mut HashSet<i32>, changed: &mut HashSet<i32>) {
        placed.insert(id);
        if !self.place_node(id, changed) {
            return;
        }
        let mut pending: Vec<i32> = self.children_of(id).collect();
        while let Some(id) = pending.pop() {
            placed.insert(id);
            self.place_node(id, changed);
            pending.extend(self.children_of(id));
        }
    }

    /// Places one node, telling whether its content moved, which moves every
    /// node below it that is placed relative to it.
    fn place_node(&mut self, id: i32, changed: &mut HashSet<i32>) -> bool {
        let (content, bounds) = self.place(id);
        let node = self
            .nodes
            .get_mut(&id)
            .expect("placed nodes are in the tree");
        if node.bounds != bounds {
            node.bounds = bounds;
            changed.insert(id);
        }
        let moved = node.content != content;
        node.content = content;
        moved
    }

    /// The node's content space and box in the document.
    fn place(&self, id: i32) -> (Affine, Option<Rect>) {
        let node = &self.nodes[&id].data;
        let location = node.location;
        let container = (Some(id) != self.root)
            .then(|| {
                self.nodes
                    .get(&location.container)
                    .filter(|_| location.container != id)
                    .or_else(|| self.root.and_then(|root| self.nodes.get(&root)))
            })
            .flatten();
        let placement = container.map_or(Affine::IDENTITY, |container| container.content)
            * transform(&location);
        let origin = Vec2::new(f64::from(location.x), f64::from(location.y));
        let scroll = if Some(id) == self.root {
            Vec2::ZERO
        } else {
            scroll(node.int(attribute::SCROLL_X), node.int(attribute::SCROLL_Y))
        };
        let content = placement * Affine::translate(origin - scroll);
        let size = (location.width > 0.0 || location.height > 0.0).then(|| {
            Rect::new(
                origin.x,
                origin.y,
                origin.x + f64::from(location.width),
                origin.y + f64::from(location.height),
            )
        });
        (
            content,
            size.map(|rect| placement.transform_rect_bbox(rect)),
        )
    }

    /// How far the frame's document is scrolled.
    pub fn document_scroll(&self) -> Vec2 {
        self.root
            .and_then(|root| self.nodes.get(&root))
            .map_or(Vec2::ZERO, |root| {
                scroll(
                    root.data.int(attribute::SCROLL_X),
                    root.data.int(attribute::SCROLL_Y),
                )
            })
    }
}

fn scroll(x: Option<i32>, y: Option<i32>) -> Vec2 {
    match (x, y) {
        (Some(x), Some(y)) => Vec2::new(f64::from(x), f64::from(y)),
        _ => Vec2::ZERO,
    }
}

fn transform(location: &AxLocation) -> Affine {
    location.transform.map_or(Affine::IDENTITY, |coefficients| {
        Affine::new(coefficients.map(f64::from))
    })
}
