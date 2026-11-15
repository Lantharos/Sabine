mod draw;
mod field;
mod input;
mod layout;

use std::collections::VecDeque;
use std::io::Write;

use crate::bridge::DIALOG_ANSWER_LINE;
use crate::osr::host::native::OsrNativeHost;
use crate::osr::host::page::BridgeRequest;
use crate::osr::protocol::{JsDialogKind, JsDialogRequest, encode_component};
use crate::{DialogAnswer, MessageDialog};
use field::TextField;
use layout::SheetLayout;

/// The dialogs waiting to show over the page, and the one it shows. A sheet
/// keeps the page from input until someone answers it; the others wait
/// their turn.
#[derive(Default)]
pub(in crate::osr::host) struct Sheets {
    queue: VecDeque<SheetRequest>,
    open: Option<OpenSheet>,
}

impl Sheets {
    pub(in crate::osr::host) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Whether a prompt holds the input method in place of the page.
    pub(in crate::osr::host) fn holds_ime(&self) -> bool {
        self.open.as_ref().is_some_and(|open| open.field.is_some())
    }
}

/// Who waits for a dialog's answer.
enum SheetOrigin {
    /// Chromium, for a page's `alert`, `confirm`, `prompt` or leave prompt.
    Js(u32),
    /// A page's `dialog` call.
    Page {
        browser_id: String,
        request_id: String,
    },
    /// The app, through `BridgeEventEmitter::show_message`.
    App(u64),
}

struct SheetRequest {
    origin: SheetOrigin,
    dialog: MessageDialog,
}

/// What on a sheet takes keys and clicks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Field,
    Button(usize),
}

struct OpenSheet {
    origin: SheetOrigin,
    dialog: MessageDialog,
    field: Option<TextField>,
    focus: Target,
    hovered: Option<Target>,
    pressed: Option<Target>,
    /// Whether the pointer drags out a selection in the field.
    selecting: bool,
    layout: Option<SheetLayout>,
    /// Whether the field took the input method over from the page.
    ime_lent: bool,
    /// Whether the keyboard moved focus, which then shows around buttons.
    focus_visible: bool,
}

impl OsrNativeHost {
    pub(in crate::osr::host) fn show_js_dialog(&mut self, request: JsDialogRequest) {
        let title = match url::Url::parse(&request.origin) {
            Ok(origin) if request.guest => origin
                .host_str()
                .map_or_else(|| self.config.title.to_string(), str::to_string),
            _ => self.config.title.to_string(),
        };
        let dialog = match request.kind {
            JsDialogKind::Alert => MessageDialog::new(title).message(request.message),
            JsDialogKind::Confirm => MessageDialog::new(title)
                .message(request.message)
                .buttons(["Cancel", "OK"]),
            JsDialogKind::Prompt => MessageDialog::new(title)
                .message(request.message)
                .buttons(["Cancel", "OK"])
                .input(request.value, ""),
            JsDialogKind::BeforeUnload => MessageDialog::new("Leave this page?")
                .message("Changes you made may not be saved.")
                .buttons(["Stay", "Leave"]),
            JsDialogKind::BeforeReload => MessageDialog::new("Reload this page?")
                .message("Changes you made may not be saved.")
                .buttons(["Cancel", "Reload"]),
        };
        self.queue_sheet(SheetOrigin::Js(request.id), dialog);
    }

    /// Takes down a JavaScript dialog Chromium no longer waits for.
    pub(in crate::osr::host) fn drop_js_dialog(&mut self, id: u32) {
        self.sheets
            .queue
            .retain(|request| !matches!(request.origin, SheetOrigin::Js(queued) if queued == id));
        if self
            .sheets
            .open
            .as_ref()
            .is_some_and(|open| matches!(open.origin, SheetOrigin::Js(open) if open == id))
        {
            self.close_sheet();
            self.open_next_sheet();
        }
    }

    /// Takes down the dialogs of pages whose browser went away.
    pub(in crate::osr::host) fn drop_page_sheets(&mut self) {
        let from_app = |origin: &SheetOrigin| matches!(origin, SheetOrigin::App(_));
        self.sheets
            .queue
            .retain(|request| from_app(&request.origin));
        if self
            .sheets
            .open
            .as_ref()
            .is_some_and(|open| !from_app(&open.origin))
        {
            self.close_sheet();
            self.open_next_sheet();
        }
    }

    pub(in crate::osr::host) fn show_page_dialog(&mut self, request: &BridgeRequest) {
        match serde_json::from_str::<MessageDialog>(request.payload) {
            Ok(dialog) => self.queue_sheet(
                SheetOrigin::Page {
                    browser_id: request.browser_id.to_string(),
                    request_id: request.request_id.to_string(),
                },
                dialog,
            ),
            Err(error) => self.send_bridge_response(
                request.browser_id,
                request.request_id,
                Err(error.to_string()),
            ),
        }
    }

    pub(in crate::osr::host) fn show_app_dialog(&mut self, id: u64, dialog: MessageDialog) {
        self.queue_sheet(SheetOrigin::App(id), dialog);
    }

    fn queue_sheet(&mut self, origin: SheetOrigin, mut dialog: MessageDialog) {
        if dialog.buttons.is_empty() {
            dialog.buttons.push("OK".into());
        }
        let last = dialog.buttons.len() - 1;
        dialog.default_button = dialog.default_button.min(last);
        dialog.cancel_button = dialog.cancel_button.min(last);
        self.sheets.queue.push_back(SheetRequest { origin, dialog });
        if !self.sheets.is_open() {
            self.open_next_sheet();
        }
    }

    fn open_next_sheet(&mut self) {
        let Some(SheetRequest { origin, dialog }) = self.sheets.queue.pop_front() else {
            return;
        };
        self.cancel_context_menu();
        self.send_control("capture_lost\n");
        let field = dialog
            .input
            .as_ref()
            .map(|input| TextField::new(input.value.clone()));
        self.sheets.open = Some(OpenSheet {
            focus: if field.is_some() {
                Target::Field
            } else {
                Target::Button(dialog.default_button)
            },
            origin,
            dialog,
            field,
            hovered: None,
            pressed: None,
            selecting: false,
            layout: None,
            ime_lent: false,
            focus_visible: false,
        });
        self.set_native_cursor(winit::cursor::CursorIcon::Default);
        self.lay_out_sheet();
        self.request_sheet_redraw();
    }

    /// Answers the open sheet with the button at `index`.
    fn answer_sheet(&mut self, button: usize) {
        let Some(open) = self.close_sheet() else {
            return;
        };
        let value = open
            .field
            .filter(|_| button != open.dialog.cancel_button)
            .map(|field| field.text);
        match open.origin {
            SheetOrigin::Js(id) => {
                let accepted = u8::from(button == open.dialog.default_button);
                let text = encode_component(value.as_deref().unwrap_or_default());
                self.send_control(format!("js_dialog\t{id}\t{accepted}\t{text}\n"));
            }
            SheetOrigin::Page {
                browser_id,
                request_id,
            } => self.send_bridge_response(
                &browser_id,
                &request_id,
                Ok(serde_json::json!(DialogAnswer { button, value })),
            ),
            SheetOrigin::App(id) => {
                let mut output = std::io::stdout().lock();
                let _ = writeln!(
                    output,
                    "{DIALOG_ANSWER_LINE}\t{id}\t{}",
                    serde_json::json!(DialogAnswer { button, value })
                );
                let _ = output.flush();
            }
        }
        self.open_next_sheet();
    }

    /// Takes the open sheet down, handing the page its input back.
    fn close_sheet(&mut self) -> Option<OpenSheet> {
        let open = self.sheets.open.take()?;
        if open.ime_lent {
            self.return_ime();
        }
        self.clear_native_cursor();
        self.request_sheet_redraw();
        Some(open)
    }

    fn request_sheet_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
