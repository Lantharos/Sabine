use std::{io::Write, path::PathBuf, sync::atomic::Ordering, thread};

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
use crate::clipboard::DropEvent;

const URI_LIST: &str = "text/uri-list";

/// A drag over the window. Only drags carrying files are accepted, and their
/// paths are read as soon as they enter.
pub(super) struct Drag {
    id: u64,
    offer: WlDataOffer,
    files: bool,
    x: f64,
    y: f64,
    action: Option<DndAction>,
    paths: Option<Vec<PathBuf>>,
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
        let files = self.surface.as_ref() == Some(&surface.id())
            && types.iter().any(|mime| mime == URI_LIST);
        offer.accept(serial, files.then(|| URI_LIST.to_string()));
        if offer.version() >= 3 {
            let (actions, preferred) =
                match (files, self.channels.outgoing_drag.load(Ordering::Relaxed)) {
                    (false, _) => (WlDndAction::empty(), WlDndAction::empty()),
                    (true, true) => (WlDndAction::Copy | WlDndAction::Move, WlDndAction::Move),
                    (true, false) => (WlDndAction::Copy, WlDndAction::Copy),
                };
            offer.set_actions(actions, preferred);
        }
        self.next_drag += 1;
        let id = self.next_drag;
        if files {
            let connection = self.connection.clone();
            let source = Offer::Data(offer.clone());
            let commands = self.channels.commands.clone();
            let mut wake = self.channels.wake.try_clone().ok();
            thread::spawn(move || {
                let plan = vec![(URI_LIST.to_string(), URI_LIST.to_string())];
                let paths = receive(&connection, &source, plan)
                    .map(|content| {
                        content
                            .items()
                            .flat_map(|(_, bytes)| file_paths(bytes))
                            .collect()
                    })
                    .unwrap_or_default();
                let _ = commands.send(Command::DragData { drag: id, paths });
                if let Some(wake) = &mut wake {
                    let _ = wake.write(&[1]);
                }
            });
        }
        self.drag = Some(Drag {
            id,
            offer,
            files,
            x,
            y,
            action: None,
            paths: None,
            dropped: false,
        });
    }

    pub(super) fn drag_motion(&mut self, x: f64, y: f64) {
        let Some(drag) = self.drag.as_mut().filter(|drag| drag.files) else {
            return;
        };
        drag.x = x;
        drag.y = y;
        if drag.paths.is_some() {
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
        if !drag.files {
            self.drag.take().expect("checked above").offer.destroy();
            return;
        }
        drag.dropped = true;
        if drag.paths.is_some() {
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
        if drag.paths.is_some() {
            self.emit(DropEvent::Leave);
        }
    }

    pub(super) fn receive_drag_data(&mut self, id: u64, paths: Vec<PathBuf>) {
        let Some(drag) = self.drag.as_mut().filter(|drag| drag.id == id) else {
            return;
        };
        if paths.is_empty() {
            drag.files = false;
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
            paths: paths.clone(),
            x: drag.x,
            y: drag.y,
        };
        drag.paths = Some(paths);
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

fn file_paths(uri_list: &[u8]) -> Vec<PathBuf> {
    String::from_utf8_lossy(uri_list)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| url::Url::parse(line).ok()?.to_file_path().ok())
        .collect()
}
