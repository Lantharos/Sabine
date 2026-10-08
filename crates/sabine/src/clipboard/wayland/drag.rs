use std::{io::Write, sync::atomic::Ordering, thread};

use wayland_client::{
    Proxy,
    protocol::{
        wl_data_device_manager::DndAction as WlDndAction, wl_data_offer::WlDataOffer,
        wl_surface::WlSurface,
    },
};
use winit::event_loop::DndAction;

use super::Command;
use super::objects::{Offer, OfferTypes};
use super::state::{State, receive};
use crate::clipboard::content::{TEXT, read_plan};
use crate::clipboard::{ClipboardContent, DropEvent};
use crate::osr::protocol::DragContent;

const URI_LIST: &str = "text/uri-list";
const HTML: &str = "text/html";

/// A drag over the window. Drags carrying files, text or HTML are accepted,
/// and their content is read as soon as they enter.
pub(super) struct Drag {
    id: u64,
    offer: WlDataOffer,
    accepted: bool,
    x: f64,
    y: f64,
    action: Option<DndAction>,
    content: Option<DragContent>,
    dropped: bool,
}

impl State {
    pub(super) fn drag_enter(
        &mut self,
        serial: u32,
        surface: &WlSurface,
        x: f64,
        y: f64,
        offer: WlDataOffer,
    ) {
        if let Some(previous) = self.drag.take() {
            previous.offer.destroy();
        }
        let types = offer
            .data::<OfferTypes>()
            .map(OfferTypes::get)
            .unwrap_or_default();
        let plan = if self.surface.as_ref() == Some(&surface.id()) {
            read_plan(&types, Some(&[URI_LIST, TEXT, HTML].map(String::from)))
        } else {
            Vec::new()
        };
        let accepted = !plan.is_empty();
        offer.accept(serial, plan.first().map(|(mime, _)| mime.clone()));
        if offer.version() >= 3 {
            let (actions, preferred) = match (
                accepted,
                self.channels.outgoing_drag.load(Ordering::Relaxed),
            ) {
                (false, _) => (WlDndAction::empty(), WlDndAction::empty()),
                (true, true) => (WlDndAction::Copy | WlDndAction::Move, WlDndAction::Move),
                (true, false) => (WlDndAction::Copy, WlDndAction::Copy),
            };
            offer.set_actions(actions, preferred);
        }
        self.next_drag += 1;
        let id = self.next_drag;
        if accepted {
            let connection = self.connection.clone();
            let source = Offer::Data(offer.clone());
            let commands = self.channels.commands.clone();
            let mut wake = self.channels.wake.try_clone().ok();
            thread::spawn(move || {
                let content = receive(&connection, &source, plan)
                    .map(|content| drag_content(&content))
                    .unwrap_or_default();
                let _ = commands.send(Command::DragData { drag: id, content });
                if let Some(wake) = &mut wake {
                    let _ = wake.write(&[1]);
                }
            });
        }
        self.drag = Some(Drag {
            id,
            offer,
            accepted,
            x,
            y,
            action: None,
            content: None,
            dropped: false,
        });
    }

    pub(super) fn drag_motion(&mut self, x: f64, y: f64) {
        let Some(drag) = self.drag.as_mut().filter(|drag| drag.accepted) else {
            return;
        };
        drag.x = x;
        drag.y = y;
        if drag.content.is_some() {
            self.emit(DropEvent::Motion { x, y });
        }
    }

    pub(super) fn drag_action(&mut self, offer: &WlDataOffer, action: WlDndAction) {
        if let Some(drag) = self.drag.as_mut().filter(|drag| drag.offer == *offer) {
            drag.action = if action.contains(WlDndAction::Move) {
                Some(DndAction::Move)
            } else if action.contains(WlDndAction::Copy) {
                Some(DndAction::Copy)
            } else {
                None
            };
        }
    }

    pub(super) fn drag_drop(&mut self) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        if !drag.accepted {
            self.drag.take().expect("checked above").offer.destroy();
            return;
        }
        drag.dropped = true;
        if drag.content.is_some() {
            self.finish_drop();
        }
    }

    pub(super) fn drag_leave(&mut self) {
        if self.drag.as_ref().is_some_and(|drag| drag.dropped) {
            return;
        }
        let Some(drag) = self.drag.take() else {
            return;
        };
        drag.offer.destroy();
        if drag.content.is_some() {
            self.emit(DropEvent::Leave);
        }
    }

    pub(super) fn receive_drag_data(&mut self, id: u64, content: DragContent) {
        let Some(drag) = self.drag.as_mut().filter(|drag| drag.id == id) else {
            return;
        };
        if content.is_empty() {
            drag.accepted = false;
            if drag.offer.version() >= 3 {
                drag.offer
                    .set_actions(WlDndAction::empty(), WlDndAction::empty());
            }
            if drag.dropped {
                self.drag.take().expect("checked above").offer.destroy();
            }
            return;
        }
        let event = DropEvent::Enter {
            content: content.clone(),
            x: drag.x,
            y: drag.y,
        };
        drag.content = Some(content);
        let dropped = drag.dropped;
        self.emit(event);
        if dropped {
            self.finish_drop();
        }
    }

    fn finish_drop(&mut self) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        self.emit(DropEvent::Drop);
        if drag.action.is_some() && drag.offer.version() >= 3 {
            drag.offer.finish();
        }
        drag.offer.destroy();
    }

    fn emit(&self, event: DropEvent) {
        if self.channels.drops.send(event).is_ok() {
            (self.channels.waker)();
        }
    }
}

fn drag_content(content: &ClipboardContent) -> DragContent {
    let mut drag = DragContent::default();
    for (mime, bytes) in content.items() {
        let text = String::from_utf8_lossy(bytes);
        match mime {
            URI_LIST => drag.add_uris(
                text.lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#')),
            ),
            TEXT => drag.text = text.into_owned(),
            HTML => drag.html = text.into_owned(),
            _ => {}
        }
    }
    drag
}
