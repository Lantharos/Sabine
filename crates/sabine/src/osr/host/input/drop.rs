use winit::{
    data_transfer::{DataTransferId, TypeHint, TypedData},
    dpi::PhysicalPosition,
    event_loop::ActiveEventLoop,
};

#[cfg(target_os = "linux")]
use crate::clipboard::DropEvent;
use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::DragContent;

use super::drag::{DRAG_OPERATIONS_ANY, dnd_actions, drag_cursor};

const FETCHED_TYPES: [TypeHint; 3] = [TypeHint::UriList, TypeHint::Plaintext, TypeHint::Html];

/// A drag from the desktop over the window. The page sees it once all of its
/// content has arrived, as browsers only announce drags with their data.
pub(in crate::osr::host) struct IncomingDrag {
    id: Option<DataTransferId>,
    content: DragContent,
    pending: usize,
    x: f32,
    y: f32,
    entered: bool,
    dropped: bool,
}

impl OsrNativeHost {
    pub(super) fn drag_entered(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        id: DataTransferId,
        position: Option<PhysicalPosition<f64>>,
    ) {
        let (x, y) = position.map_or((self.cursor_x, self.cursor_y), |position| {
            self.logical_point(position)
        });
        let _ = event_loop.set_valid_dnd_actions(id, &dnd_actions(DRAG_OPERATIONS_ANY));
        if self.drag.outgoing() == Some(id) {
            self.drag.incoming = Some(IncomingDrag::entered(Some(id), x, y));
            self.send_drag_position("drag_enter_source", x, y, None);
            return;
        }
        let Ok(transfer) = event_loop.data_transfer(id) else {
            return;
        };
        let pending = FETCHED_TYPES
            .iter()
            .filter(|hint| transfer.has_type(*hint))
            .filter(|hint| event_loop.fetch_data_transfer(id, *hint).is_ok())
            .count();
        if pending == 0 {
            let _ = event_loop.set_valid_dnd_actions(id, &[]);
            return;
        }
        self.drag.incoming = Some(IncomingDrag {
            id: Some(id),
            content: DragContent::default(),
            pending,
            x,
            y,
            entered: false,
            dropped: false,
        });
    }

    pub(super) fn drag_received(&mut self, id: DataTransferId, value: &dyn TypedData) {
        let Some(drag) = self.incoming_drag(id) else {
            return;
        };
        match value.type_().hint() {
            Some(TypeHint::UriList) => drag.content.add_uris(
                value
                    .try_as_uris()
                    .unwrap_or_default()
                    .iter()
                    .map(String::as_str),
            ),
            Some(TypeHint::Plaintext) => {
                drag.content.text = value.try_as_string().unwrap_or_default()
            }
            Some(TypeHint::Html) => {
                drag.content.html = value
                    .try_as_bytes()
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .unwrap_or_default();
            }
            _ => {}
        }
        drag.pending = drag.pending.saturating_sub(1);
        if drag.pending == 0 {
            self.enter_incoming_drag();
        }
    }

    pub(super) fn drag_moved(&mut self, id: DataTransferId, position: PhysicalPosition<f64>) {
        let (x, y) = self.logical_point(position);
        if self.incoming_drag(id).is_some_and(|drag| drag.entered) {
            self.move_incoming_drag(x, y);
        } else if let Some(drag) = self.incoming_drag(id) {
            drag.x = x;
            drag.y = y;
        }
    }

    pub(super) fn drag_dropped(&mut self, id: DataTransferId) {
        let Some(drag) = self.incoming_drag(id) else {
            return;
        };
        drag.dropped = true;
        if drag.entered {
            self.drop_incoming_drag();
        }
    }

    pub(super) fn drag_left(&mut self, id: DataTransferId) {
        if self.incoming_drag(id).is_some() {
            self.leave_incoming_drag();
        }
    }

    /// Tells the desktop which drop the page under the pointer would accept.
    pub(in crate::osr::host) fn update_drag_operation(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        operation: u32,
    ) {
        if self.drag.inside_window() {
            self.set_native_cursor(drag_cursor(operation));
        }
        if let Some(id) = self.drag.incoming.as_ref().and_then(|drag| drag.id) {
            let _ = event_loop.set_valid_dnd_actions(id, &dnd_actions(operation));
        }
    }

    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn deliver_clipboard_drops(&mut self) {
        let Some(clipboard) = &self.clipboard else {
            return;
        };
        let events = clipboard.drop_events().collect::<Vec<_>>();
        for event in events {
            match event {
                DropEvent::Enter { content, x, y } => {
                    let (x, y) = (x as f32, y as f32);
                    let mut drag = IncomingDrag::entered(None, x, y);
                    if self.drag.outgoing().is_some() {
                        self.drag.incoming = Some(drag);
                        self.send_drag_position("drag_enter_source", x, y, None);
                    } else {
                        drag.content = content;
                        self.drag.incoming = Some(drag);
                        self.enter_incoming_drag();
                    }
                }
                DropEvent::Motion { x, y } if self.drag.incoming.is_some() => {
                    self.move_incoming_drag(x as f32, y as f32);
                }
                DropEvent::Drop if self.drag.incoming.is_some() => self.drop_incoming_drag(),
                DropEvent::Leave if self.drag.incoming.is_some() => self.leave_incoming_drag(),
                _ => {}
            }
        }
    }

    pub(in crate::osr::host) fn clipboard_owns_drops(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.clipboard
                .as_ref()
                .is_some_and(|clipboard| clipboard.owns_drops())
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    fn incoming_drag(&mut self, id: DataTransferId) -> Option<&mut IncomingDrag> {
        self.drag
            .incoming
            .as_mut()
            .filter(|drag| drag.id == Some(id))
    }

    fn enter_incoming_drag(&mut self) {
        let Some(drag) = self.drag.incoming.as_mut() else {
            return;
        };
        drag.entered = true;
        let (x, y, dropped) = (drag.x, drag.y, drag.dropped);
        let content = serde_json::to_string(&drag.content).unwrap_or_default();
        let (x, y) = self.clamped_content_position(x, y);
        self.send_control(format!(
            "drag_enter\t{x:.0}\t{y:.0}\t{}\t{DRAG_OPERATIONS_ANY}\t{content}\n",
            self.input_modifiers()
        ));
        if dropped {
            self.drop_incoming_drag();
        }
    }

    fn move_incoming_drag(&mut self, x: f32, y: f32) {
        if let Some(drag) = &mut self.drag.incoming {
            drag.x = x;
            drag.y = y;
        }
        let operations = self.drag.page_operations().unwrap_or(DRAG_OPERATIONS_ANY);
        self.send_drag_position("drag_over", x, y, Some(operations));
    }

    fn drop_incoming_drag(&mut self) {
        if let Some(drag) = self.drag.incoming.take() {
            self.send_drag_position("drag_drop", drag.x, drag.y, None);
        }
    }

    fn leave_incoming_drag(&mut self) {
        if self.drag.incoming.take().is_some_and(|drag| drag.entered) {
            self.send_control("drag_leave\n");
        }
    }
}

impl IncomingDrag {
    fn entered(id: Option<DataTransferId>, x: f32, y: f32) -> Self {
        Self {
            id,
            content: DragContent::default(),
            pending: 0,
            x,
            y,
            entered: true,
            dropped: false,
        }
    }
}
