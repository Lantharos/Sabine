use std::path::PathBuf;

use winit::{
    data_transfer::{DataTransferSendBuilder, SendData, TypeHint},
    event_loop::{ActiveEventLoop, DndAction},
};

#[cfg(target_os = "linux")]
use crate::clipboard::DropEvent;
use crate::osr::host::native::{IncomingFileDrag, OsrNativeHost};
use crate::osr::protocol::FileDragRequest;

impl OsrNativeHost {
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

    #[cfg(target_os = "linux")]
    pub(in crate::osr::host) fn deliver_clipboard_drops(&mut self) {
        let Some(clipboard) = &self.clipboard else {
            return;
        };
        let events = clipboard.drop_events().collect::<Vec<_>>();
        for event in events {
            match event {
                DropEvent::Enter { paths, x, y } => {
                    self.incoming_file_drag = Some(IncomingFileDrag {
                        id: None,
                        paths,
                        x: x as f32,
                        y: y as f32,
                        action: None,
                        entered: true,
                        dropped: false,
                    });
                    self.emit_incoming_file_drag("enter");
                }
                DropEvent::Motion { x, y, action } => {
                    if let Some(drag) = &mut self.incoming_file_drag {
                        drag.x = x as f32;
                        drag.y = y as f32;
                        drag.action = action;
                        self.emit_incoming_file_drag("over");
                    }
                }
                DropEvent::Drop { action } => {
                    if let Some(drag) = &mut self.incoming_file_drag {
                        drag.action = action;
                        self.emit_incoming_file_drag("drop");
                        self.incoming_file_drag = None;
                    }
                }
                DropEvent::Leave => {
                    if self.incoming_file_drag.is_some() {
                        self.emit_incoming_file_drag("leave");
                        self.incoming_file_drag = None;
                    }
                }
            }
        }
    }

    pub(in crate::osr::host) fn begin_incoming_file_drag(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        id: winit::data_transfer::DataTransferId,
        position: Option<winit::dpi::PhysicalPosition<f64>>,
    ) {
        let Ok(transfer) = event_loop.data_transfer(id) else {
            return;
        };
        if !transfer.has_type(&TypeHint::UriList) {
            let _ = event_loop.set_valid_dnd_actions(id, &[]);
            return;
        }
        let actions: &[DndAction] = if self.active_file_drag == Some(id) {
            &[DndAction::Copy, DndAction::Move]
        } else {
            &[DndAction::Copy]
        };
        if event_loop.set_valid_dnd_actions(id, actions).is_err() {
            return;
        }
        let (x, y) = position
            .map(|position| self.logical_drag_position(position))
            .unwrap_or((self.cursor_x, self.cursor_y));
        self.incoming_file_drag = Some(IncomingFileDrag {
            id: Some(id),
            paths: Vec::new(),
            x,
            y,
            action: None,
            entered: false,
            dropped: false,
        });
        if event_loop
            .fetch_data_transfer(id, &TypeHint::UriList)
            .is_err()
        {
            self.incoming_file_drag = None;
            let _ = event_loop.set_valid_dnd_actions(id, &[]);
        }
    }

    pub(in crate::osr::host) fn update_incoming_file_drag(
        &mut self,
        id: winit::data_transfer::DataTransferId,
        position: winit::dpi::PhysicalPosition<f64>,
        action: Option<DndAction>,
    ) {
        let (x, y) = self.logical_drag_position(position);
        let Some(drag) = self
            .incoming_file_drag
            .as_mut()
            .filter(|drag| drag.id == Some(id))
        else {
            return;
        };
        drag.x = x;
        drag.y = y;
        drag.action = action;
        if drag.entered {
            self.emit_incoming_file_drag("over");
        }
    }

    pub(in crate::osr::host) fn receive_incoming_file_drag(
        &mut self,
        id: winit::data_transfer::DataTransferId,
        value: &dyn winit::data_transfer::TypedData,
    ) {
        let Some(drag) = self
            .incoming_file_drag
            .as_mut()
            .filter(|drag| drag.id == Some(id))
        else {
            return;
        };
        let Ok(uris) = value.try_as_uris() else {
            return;
        };
        drag.paths = uris
            .iter()
            .filter_map(|uri| url::Url::parse(uri).ok()?.to_file_path().ok())
            .collect();
        if drag.paths.is_empty() {
            self.incoming_file_drag = None;
            return;
        }
        drag.entered = true;
        let dropped = drag.dropped;
        self.emit_incoming_file_drag(if dropped { "drop" } else { "enter" });
        if dropped {
            self.incoming_file_drag = None;
        }
    }

    pub(in crate::osr::host) fn drop_incoming_file_drag(
        &mut self,
        id: winit::data_transfer::DataTransferId,
        action: Option<DndAction>,
    ) {
        let Some(drag) = self
            .incoming_file_drag
            .as_mut()
            .filter(|drag| drag.id == Some(id))
        else {
            return;
        };
        drag.action = action;
        drag.dropped = true;
        if drag.entered {
            self.emit_incoming_file_drag("drop");
            self.incoming_file_drag = None;
        }
    }

    pub(in crate::osr::host) fn leave_incoming_file_drag(
        &mut self,
        id: winit::data_transfer::DataTransferId,
    ) {
        if self
            .incoming_file_drag
            .as_ref()
            .is_some_and(|drag| drag.id == Some(id) && drag.entered)
        {
            self.emit_incoming_file_drag("leave");
        }
        if self
            .incoming_file_drag
            .as_ref()
            .is_some_and(|drag| drag.id == Some(id))
        {
            self.incoming_file_drag = None;
        }
    }

    fn logical_drag_position(&self, position: winit::dpi::PhysicalPosition<f64>) -> (f32, f32) {
        let scale = self.scale_factor.max(1.0) as f32;
        (position.x as f32 / scale, position.y as f32 / scale)
    }

    fn emit_incoming_file_drag(&self, phase: &str) {
        let Some(drag) = self.incoming_file_drag.as_ref() else {
            return;
        };
        let (x, y) = self
            .content_position(drag.x, drag.y)
            .unwrap_or((drag.x, drag.y));
        let internal = self.active_file_drag.is_some();
        let action = if internal {
            if self.modifiers.control_key() || self.modifiers.meta_key() {
                "copy"
            } else {
                "move"
            }
        } else {
            match drag.action {
                Some(DndAction::Copy) => "copy",
                Some(DndAction::Move) => "move",
                Some(DndAction::Link) => "link",
                _ => "none",
            }
        };
        let payload = serde_json::json!({
            "phase": phase,
            "paths": drag.paths,
            "x": x,
            "y": y,
            "action": action,
            "internal": internal,
        });
        self.send_control(&format!("file_drag\t{payload}\n"));
    }

    pub(in crate::osr::host) fn start_file_drag(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        request: FileDragRequest,
    ) {
        let FileDragRequest { paths } = request;
        let Some(window) = self.window.clone() else {
            self.finish_file_drag(None);
            return;
        };

        let paths: Vec<PathBuf> = paths
            .into_iter()
            .map(|path| PathBuf::from(path.trim()))
            .filter(|path| !path.as_os_str().is_empty())
            .collect();
        if paths.is_empty() {
            self.finish_file_drag(None);
            return;
        }

        let transfer = DataTransferSendBuilder::new(paths)
            .with_type(TypeHint::UriList, |paths, _| {
                paths
                    .iter()
                    .map(|path| url::Url::from_file_path(path).map(String::from))
                    .collect::<Result<Vec<_>, _>>()
                    .ok()
                    .map(SendData::Uris)
            })
            .build();

        match event_loop.start_drag(
            window.id(),
            transfer,
            &[DndAction::Copy, DndAction::Move],
            None,
        ) {
            Ok(id) => {
                self.active_file_drag = Some(id);
                #[cfg(target_os = "linux")]
                if let Some(clipboard) = &self.clipboard {
                    clipboard.set_outgoing_drag(true);
                }
            }
            Err(error) => {
                eprintln!("failed to start native file drag: {error}");
                self.finish_file_drag(None);
            }
        }
    }

    pub(in crate::osr::host) fn finish_file_drag(&mut self, action: Option<DndAction>) {
        self.active_file_drag = None;
        #[cfg(target_os = "linux")]
        if let Some(clipboard) = &self.clipboard {
            clipboard.set_outgoing_drag(false);
        }
        let operation = match action {
            Some(DndAction::Copy) => "copy",
            Some(DndAction::Move) => "move",
            Some(DndAction::Link) => "link",
            _ => "none",
        };
        let (x, y) = self
            .content_position(self.cursor_x, self.cursor_y)
            .unwrap_or((self.cursor_x, self.cursor_y));
        self.send_control(&format!(
            "file_drag_ended\t{:.0}\t{:.0}\t{operation}\n",
            x, y
        ));
    }
}
