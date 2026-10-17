use std::collections::HashMap;

use sabine_platform::{PlatformEvent, TrayActivation, TrayMenuItem, TrayMenuItemKind};
use serde::Serialize;
use zbus::{OwnedVariant, Structure, Type, Variant, fdo, interface, object_server::SignalEmitter};

use crate::desktop::EventQueue;

const ROOT: i32 = 0;

type Properties = HashMap<String, OwnedVariant>;

/// One entry of a `GetLayout` answer: an item, what it shows and its
/// children, each a variant holding another layout.
#[derive(Serialize, Type)]
pub(super) struct Layout {
    id: i32,
    properties: Properties,
    children: Vec<Variant<'static>>,
}

impl From<Layout> for Variant<'static> {
    fn from(layout: Layout) -> Self {
        Variant::from(
            Structure::builder()
                .add_field(layout.id)
                .add_field(layout.properties)
                .add_field(layout.children)
                .build()
                .expect("a menu layout is a valid structure"),
        )
    }
}

struct Node {
    item_id: String,
    action: Option<String>,
    label: String,
    enabled: bool,
    kind: Kind,
    children: Vec<i32>,
}

enum Kind {
    Normal,
    Separator,
    Checkbox(bool),
    Submenu,
}

/// The app's `com.canonical.dbusmenu`, numbering the tray's menu items in
/// order with the root as 0. Every new menu is a new revision.
pub(super) struct Menu {
    tray_id: String,
    events: EventQueue,
    nodes: Vec<Node>,
    revision: u32,
}

impl Menu {
    pub(super) fn new(tray_id: String, events: EventQueue, items: &[TrayMenuItem]) -> Self {
        let mut menu = Self {
            tray_id,
            events,
            nodes: Vec::new(),
            revision: 0,
        };
        menu.show(items);
        menu
    }

    /// Replaces the menu, returning its new revision.
    pub(super) fn show(&mut self, items: &[TrayMenuItem]) -> u32 {
        self.nodes = vec![Node {
            item_id: String::new(),
            action: None,
            label: String::new(),
            enabled: true,
            kind: Kind::Submenu,
            children: Vec::new(),
        }];
        self.nodes[0].children = self.add(items);
        self.revision += 1;
        self.revision
    }

    fn add(&mut self, items: &[TrayMenuItem]) -> Vec<i32> {
        items
            .iter()
            .map(|item| {
                let id = self.nodes.len() as i32;
                let kind = match &item.kind {
                    TrayMenuItemKind::Normal => Kind::Normal,
                    TrayMenuItemKind::Separator => Kind::Separator,
                    TrayMenuItemKind::Checkbox { checked } => Kind::Checkbox(*checked),
                    TrayMenuItemKind::Submenu(_) => Kind::Submenu,
                };
                self.nodes.push(Node {
                    item_id: item.id.clone(),
                    action: item.action.clone(),
                    label: item.label.replace('_', "__"),
                    enabled: item.enabled,
                    kind,
                    children: Vec::new(),
                });
                if let TrayMenuItemKind::Submenu(children) = &item.kind {
                    let children = self.add(children);
                    self.nodes[id as usize].children = children;
                }
                id
            })
            .collect()
    }

    fn node(&self, id: i32) -> fdo::Result<&Node> {
        usize::try_from(id)
            .ok()
            .and_then(|index| self.nodes.get(index))
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("the menu has no item {id}")))
    }

    fn properties(&self, id: i32, names: &[String]) -> fdo::Result<Properties> {
        let node = self.node(id)?;
        let mut properties = HashMap::new();
        let mut set = |name: &str, value: Variant<'_>| {
            if names.is_empty() || names.iter().any(|wanted| wanted == name) {
                properties.insert(
                    name.to_string(),
                    OwnedVariant::try_from(value).expect("menu properties hold no descriptors"),
                );
            }
        };
        if id != ROOT {
            match node.kind {
                Kind::Separator => set("type", Variant::from("separator")),
                Kind::Checkbox(checked) => {
                    set("toggle-type", Variant::from("checkmark"));
                    set("toggle-state", Variant::from(i32::from(checked)));
                }
                Kind::Submenu => set("children-display", Variant::from("submenu")),
                Kind::Normal => {}
            }
            if !matches!(node.kind, Kind::Separator) {
                set("label", Variant::from(node.label.as_str()));
            }
            if !node.enabled {
                set("enabled", Variant::from(false));
            }
        }
        Ok(properties)
    }

    fn layout(&self, id: i32, depth: i32, names: &[String]) -> fdo::Result<Layout> {
        let children = if depth == 0 {
            Vec::new()
        } else {
            self.node(id)?
                .children
                .iter()
                .map(|child| self.layout(*child, depth - 1, names).map(Variant::from))
                .collect::<fdo::Result<_>>()?
        };
        Ok(Layout {
            id,
            properties: self.properties(id, names)?,
            children,
        })
    }

    /// The activation for a clicked item. A checkbox flips its state, as the
    /// menu itself shows.
    fn click(&mut self, id: i32) -> Option<TrayActivation> {
        let node = self.nodes.get_mut(usize::try_from(id).ok()?)?;
        let checked = match &mut node.kind {
            Kind::Checkbox(checked) => {
                *checked = !*checked;
                Some(*checked)
            }
            Kind::Normal => None,
            Kind::Separator | Kind::Submenu => return None,
        };
        Some(TrayActivation::item(
            self.tray_id.clone(),
            node.item_id.clone(),
            node.action.clone(),
            checked,
        ))
    }
}

#[interface(name = "com.canonical.dbusmenu")]
impl Menu {
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> fdo::Result<(u32, Layout)> {
        Ok((
            self.revision,
            self.layout(parent_id, recursion_depth, &property_names)?,
        ))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, Properties)> {
        ids.into_iter()
            .filter_map(|id| Some((id, self.properties(id, &property_names).ok()?)))
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> fdo::Result<OwnedVariant> {
        self.properties(id, std::slice::from_ref(&name))?
            .remove(&name)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("item {id} has no {name}")))
    }

    async fn event(
        &mut self,
        id: i32,
        event_id: String,
        _data: OwnedVariant,
        _timestamp: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) {
        if event_id != "clicked" {
            return;
        }
        let Some(activation) = self.click(id) else {
            return;
        };
        if activation.checked.is_some() {
            self.revision += 1;
            let _ = Self::layout_updated(&emitter, self.revision, ROOT).await;
        }
        let _ = self.events.send(PlatformEvent::Tray(activation));
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(signal)]
    pub(super) async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;
}
