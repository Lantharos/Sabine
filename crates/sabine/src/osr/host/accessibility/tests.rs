use std::collections::HashSet;

use accesskit::{NodeId, Rect};

use super::frame::{FrameChanges, FrameTree};
use super::page::PageTree;
use super::{WINDOW, node_id};
use crate::osr::protocol::accessibility::{
    AxLocation, AxNode, AxTreeChange, AxTreeData, AxTreeUpdate, AxValue, attribute,
};

fn node(id: i32, children: &[i32]) -> AxNode {
    AxNode {
        id,
        role: "genericContainer".to_string(),
        states: 0,
        actions: 0,
        location: AxLocation {
            container: -1,
            ..AxLocation::default()
        },
        children: children.to_vec(),
        attributes: Vec::new(),
    }
}

fn placed(mut node: AxNode, container: i32, x: f32, y: f32) -> AxNode {
    node.location = AxLocation {
        container,
        x,
        y,
        width: 10.0,
        height: 10.0,
        transform: None,
    };
    node
}

fn update(root: i32, nodes: Vec<AxNode>) -> AxTreeUpdate {
    AxTreeUpdate {
        data: None,
        clear: 0,
        root,
        nodes,
    }
}

fn apply(frame: &mut FrameTree, update: AxTreeUpdate) {
    frame.apply(update, &mut FrameChanges::default());
}

#[test]
fn dropped_children_take_their_subtrees_along() {
    let mut frame = FrameTree::new(1);
    apply(
        &mut frame,
        update(1, vec![node(1, &[2]), node(2, &[3]), node(3, &[])]),
    );
    apply(&mut frame, update(0, vec![node(1, &[])]));
    assert_eq!(frame.subtree().collect::<Vec<_>>(), [1]);
    assert_eq!(frame.nodes.len(), 1);
}

#[test]
fn reparented_nodes_stay() {
    let mut frame = FrameTree::new(1);
    apply(
        &mut frame,
        update(
            1,
            vec![node(1, &[2, 3]), node(2, &[]), node(3, &[4]), node(4, &[])],
        ),
    );
    apply(&mut frame, update(0, vec![node(2, &[4]), node(3, &[])]));
    assert_eq!(frame.nodes[&4].parent, Some(2));
    assert!(frame.reaches_root(4));
}

#[test]
fn nodes_sit_in_their_scrolled_containers() {
    let mut frame = FrameTree::new(1);
    let mut root = placed(node(1, &[2]), -1, 0.0, 0.0);
    root.attributes = vec![
        (attribute::SCROLL_X, AxValue::Int(0)),
        (attribute::SCROLL_Y, AxValue::Int(40)),
    ];
    let mut list = placed(node(2, &[3]), 1, 0.0, 50.0);
    list.attributes = vec![
        (attribute::SCROLL_X, AxValue::Int(0)),
        (attribute::SCROLL_Y, AxValue::Int(100)),
    ];
    let item = placed(node(3, &[]), 2, 5.0, 150.0);
    apply(&mut frame, update(1, vec![root, list, item]));
    assert_eq!(
        frame.nodes[&2].bounds,
        Some(Rect::new(0.0, 50.0, 10.0, 60.0))
    );
    assert_eq!(
        frame.nodes[&3].bounds,
        Some(Rect::new(5.0, 100.0, 15.0, 110.0))
    );

    let mut moved = HashSet::new();
    frame.move_nodes(
        vec![(2, placed(node(2, &[]), 1, 0.0, 70.0).location)],
        &mut moved,
    );
    assert_eq!(moved, HashSet::from([2, 3]));
    assert_eq!(
        frame.nodes[&3].bounds,
        Some(Rect::new(5.0, 120.0, 15.0, 130.0))
    );
}

fn tree(tree: &str, parent: &str, nodes: Vec<AxNode>) -> AxTreeChange {
    AxTreeChange {
        tree: tree.to_string(),
        updates: vec![AxTreeUpdate {
            data: Some(AxTreeData {
                parent_tree: parent.to_string(),
                focus: nodes[0].id,
                ..AxTreeData::default()
            }),
            ..update(nodes[0].id, nodes)
        }],
    }
}

fn ids(update: &accesskit::TreeUpdate) -> HashSet<NodeId> {
    update.nodes.iter().map(|(id, _)| *id).collect()
}

#[test]
fn a_frame_shows_once_its_iframe_does() {
    let mut page = PageTree::default();
    page.apply_tree(tree("child", "main", vec![node(1, &[2]), node(2, &[])]));
    page.apply_tree(tree("main", "", vec![node(1, &[])]));
    let first = page.take_update();
    assert_eq!(ids(&first), HashSet::from([WINDOW, node_id(2, 1)]));

    let mut iframe = node(5, &[]);
    iframe.attributes = vec![(attribute::CHILD_TREE, AxValue::String("child".to_string()))];
    page.apply_tree(AxTreeChange {
        tree: "main".to_string(),
        updates: vec![update(0, vec![node(1, &[5]), iframe])],
    });
    let second = page.take_update();
    assert_eq!(
        ids(&second),
        HashSet::from([node_id(2, 1), node_id(2, 5), node_id(1, 1), node_id(1, 2)])
    );
    let host = &second
        .nodes
        .iter()
        .find(|(id, _)| *id == node_id(2, 5))
        .unwrap()
        .1;
    assert_eq!(host.children(), [node_id(1, 1)]);
}
