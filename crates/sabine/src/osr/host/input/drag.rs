use winit::{
    data_transfer::{DataTransferId, DataTransferSendBuilder, SendData, TypeHint},
    event_loop::{ActiveEventLoop, DndAction},
};

use crate::osr::host::native::OsrNativeHost;
use crate::osr::protocol::DragContent;

pub(super) const DRAG_OPERATION_NONE: u32 = 0;
const DRAG_OPERATION_COPY: u32 = 1;
const DRAG_OPERATION_LINK: u32 = 2;
const DRAG_OPERATION_MOVE: u32 = 16;
pub(super) const DRAG_OPERATIONS_ANY: u32 =
    DRAG_OPERATION_COPY | DRAG_OPERATION_LINK | DRAG_OPERATION_MOVE;

/// Drags the page started, and drags from elsewhere passing over the window.
#[derive(Default)]
pub(in crate::osr::host) struct DragState {
    page: Option<PageDrag>,
    pub(super) incoming: Option<super::drop::IncomingDrag>,
}

/// A drag that stays inside the window until the pointer leaves it, and then
/// continues as a drag the desktop carries to other applications.
struct PageDrag {
    content: DragContent,
    operations: u32,
    outgoing: Option<DataTransferId>,
}

impl DragState {
    pub(super) fn outgoing(&self) -> Option<DataTransferId> {
        self.page.as_ref()?.outgoing
    }

    pub(super) fn page_operations(&self) -> Option<u32> {
        self.page.as_ref().map(|drag| drag.operations)
    }

    pub(super) fn inside_window(&self) -> bool {
        self.page
            .as_ref()
            .is_some_and(|drag| drag.outgoing.is_none())
    }
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn begin_page_drag(&mut self, content: DragContent, operations: u32) {
        self.drag.page = Some(PageDrag {
            content,
            operations,
            outgoing: None,
        });
        self.send_drag_position("drag_enter_source", self.cursor_x, self.cursor_y, None);
    }

    /// Moves a page drag that is still inside the window. Returns false when
    /// no such drag is in progress.
    pub(super) fn move_page_drag(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        x: f32,
        y: f32,
    ) -> bool {
        let Some(drag) = &self.drag.page else {
            return false;
        };
        if drag.outgoing.is_some() {
            return true;
        }
        self.cursor_x = x;
        self.cursor_y = y;
        if x >= 0.0 && y >= 0.0 && x < self.logical_width() && y < self.logical_height() {
            self.send_drag_position("drag_over", x, y, Some(drag.operations));
        } else {
            self.carry_page_drag_out(event_loop);
        }
        true
    }

    pub(super) fn carry_page_drag_out(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(drag) = self
            .drag
            .page
            .as_mut()
            .filter(|drag| drag.outgoing.is_none())
        else {
            return;
        };
        let transfer = desktop_transfer(&drag.content);
        let actions = dnd_actions(drag.operations);
        self.send_control("drag_leave\n");
        let started = self
            .window
            .as_ref()
            .ok_or_else(|| "the window is gone".to_string())
            .and_then(|window| {
                event_loop
                    .start_drag(window.id(), transfer, &actions, None)
                    .map_err(|error| error.to_string())
            });
        match started {
            Ok(id) => {
                if let Some(drag) = &mut self.drag.page {
                    drag.outgoing = Some(id);
                }
                #[cfg(target_os = "linux")]
                if let Some(clipboard) = &self.clipboard {
                    clipboard.set_outgoing_drag(true);
                }
            }
            Err(error) => {
                sabine_runtime::report_error("input", format!("could not start a drag: {error}"));
                self.end_page_drag(None);
            }
        }
    }

    /// Drops a page drag that never left the window. Returns false when no
    /// such drag is in progress.
    pub(super) fn drop_page_drag(&mut self, x: f32, y: f32) -> bool {
        if self
            .drag
            .page
            .as_ref()
            .is_none_or(|drag| drag.outgoing.is_some())
        {
            return false;
        }
        self.drag.page = None;
        self.clear_native_cursor();
        self.send_drag_position("drag_drop", x, y, None);
        true
    }

    /// Escape cancels a page drag that is still inside the window.
    pub(super) fn cancel_page_drag(&mut self) -> bool {
        if self
            .drag
            .page
            .as_ref()
            .is_none_or(|drag| drag.outgoing.is_some())
        {
            return false;
        }
        self.send_control("drag_leave\n");
        self.end_page_drag(None);
        true
    }

    pub(super) fn end_page_drag(&mut self, action: Option<DndAction>) {
        if self.drag.page.take().is_none() {
            return;
        }
        self.clear_native_cursor();
        #[cfg(target_os = "linux")]
        if let Some(clipboard) = &self.clipboard {
            clipboard.set_outgoing_drag(false);
        }
        let (x, y) = self.clamped_content_position(self.cursor_x, self.cursor_y);
        self.send_control(format!(
            "drag_source_ended\t{x:.0}\t{y:.0}\t{}\n",
            operation_for(action)
        ));
    }

    pub(super) fn send_drag_position(
        &self,
        command: &str,
        x: f32,
        y: f32,
        operations: Option<u32>,
    ) {
        let (x, y) = self.clamped_content_position(x, y);
        let modifiers = self.input_modifiers();
        let line = match operations {
            Some(operations) => format!("{command}\t{x:.0}\t{y:.0}\t{modifiers}\t{operations}\n"),
            None => format!("{command}\t{x:.0}\t{y:.0}\t{modifiers}\n"),
        };
        if command == "drag_over" {
            self.send_mouse_motion(line);
        } else {
            self.send_control(line);
        }
    }
}

fn desktop_transfer(content: &DragContent) -> Box<dyn winit::data_transfer::DataTransferSend> {
    let mut transfer = DataTransferSendBuilder::new(content.clone());
    let uris = content
        .files
        .iter()
        .filter_map(|path| url::Url::from_file_path(path).ok().map(String::from))
        .chain((!content.url.is_empty()).then(|| content.url.clone()))
        .collect::<Vec<_>>();
    if !uris.is_empty() {
        transfer.add_type(TypeHint::UriList, move |_, _| {
            Some(SendData::Uris(uris.clone()))
        });
    }
    let text = if content.text.is_empty() {
        content.url.clone()
    } else {
        content.text.clone()
    };
    if !text.is_empty() {
        transfer.add_type(TypeHint::Plaintext, move |_, _| Some(text.clone()));
    }
    if !content.html.is_empty() {
        transfer.add_type(TypeHint::Html, |content, _| Some(content.html.clone()));
    }
    transfer.build()
}

/// The pointer while a page drag is inside the window, which the desktop no
/// longer draws once the page has taken the drag.
pub(super) fn drag_cursor(operation: u32) -> winit::cursor::CursorIcon {
    use winit::cursor::CursorIcon;
    if operation & DRAG_OPERATION_MOVE != 0 {
        CursorIcon::Grabbing
    } else if operation & DRAG_OPERATION_COPY != 0 {
        CursorIcon::Copy
    } else if operation & DRAG_OPERATION_LINK != 0 {
        CursorIcon::Alias
    } else {
        CursorIcon::NoDrop
    }
}

pub(super) fn dnd_actions(operations: u32) -> Vec<DndAction> {
    [
        (DRAG_OPERATION_COPY, DndAction::Copy),
        (DRAG_OPERATION_MOVE, DndAction::Move),
        (DRAG_OPERATION_LINK, DndAction::Link),
    ]
    .into_iter()
    .filter(|(operation, _)| operations & operation != 0)
    .map(|(_, action)| action)
    .collect()
}

fn operation_for(action: Option<DndAction>) -> u32 {
    match action {
        Some(DndAction::Copy) => DRAG_OPERATION_COPY,
        Some(DndAction::Move) => DRAG_OPERATION_MOVE,
        Some(DndAction::Link) => DRAG_OPERATION_LINK,
        _ => DRAG_OPERATION_NONE,
    }
}
