use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::BridgeEventEmitter;
use super::lock;
use crate::WindowId;

/// What the window host writes when someone answers a dialog the app showed.
pub(crate) const DIALOG_ANSWER_LINE: &str = "SABINE_DIALOG_ANSWER";

/// A message the window shows over its page until someone picks one of its
/// buttons, drawn like the window's context menu.
///
/// Buttons sit in a row in the order given. Enter picks the default button
/// and Escape the cancel button. A dialog with an [`DialogInput`] asks for a
/// line of text, as `prompt()` does.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MessageDialog {
    pub title: String,
    pub message: String,
    /// The buttons' labels. A dialog without any has a single OK.
    pub buttons: Vec<String>,
    pub default_button: usize,
    pub cancel_button: usize,
    pub input: Option<DialogInput>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DialogInput {
    pub value: String,
    pub placeholder: String,
}

/// The button someone picked, and the text they entered in a dialog that
/// asks for some.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogAnswer {
    pub button: usize,
    pub value: Option<String>,
}

impl MessageDialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Sets the buttons, the last one the default and the first one the
    /// cancel button.
    pub fn buttons(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.buttons = labels.into_iter().map(Into::into).collect();
        self.default_button = self.buttons.len().saturating_sub(1);
        self.cancel_button = 0;
        self
    }

    pub fn default_button(mut self, index: usize) -> Self {
        self.default_button = index;
        self
    }

    pub fn cancel_button(mut self, index: usize) -> Self {
        self.cancel_button = index;
        self
    }

    /// Asks for a line of text, starting with `value`.
    pub fn input(mut self, value: impl Into<String>, placeholder: impl Into<String>) -> Self {
        self.input = Some(DialogInput {
            value: value.into(),
            placeholder: placeholder.into(),
        });
        self
    }
}

/// The dialogs the app showed that wait for an answer, by the window they
/// are in.
#[derive(Default)]
pub(super) struct PendingDialogs {
    next: u64,
    waiting: HashMap<u64, (WindowId, oneshot::Sender<DialogAnswer>)>,
}

impl PendingDialogs {
    /// Forgets the dialogs of a window that closed, which answers them with
    /// their cancel button.
    pub(super) fn window_closed(&mut self, window: WindowId) {
        self.waiting.retain(|_, (owner, _)| *owner != window);
    }
}

/// Resolves to the answer of a dialog, or to its cancel button when its
/// window closes first.
pub struct DialogReply {
    answer: oneshot::Receiver<DialogAnswer>,
    cancel: usize,
}

impl Future for DialogReply {
    type Output = DialogAnswer;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let cancel = self.cancel;
        Pin::new(&mut self.answer).poll(context).map(|answer| {
            answer.unwrap_or(DialogAnswer {
                button: cancel,
                value: None,
            })
        })
    }
}

impl BridgeEventEmitter {
    /// Shows `dialog` over one window's page.
    pub fn show_message(&self, window: WindowId, dialog: &MessageDialog) -> DialogReply {
        let (sender, answer) = oneshot::channel();
        let id = {
            let mut pending = lock(&self.dialogs);
            pending.next += 1;
            let id = pending.next;
            pending.waiting.insert(id, (window, sender));
            id
        };
        let mut request = serde_json::json!(dialog);
        request["id"] = id.into();
        if !self.host_control(Some(window), "dialog", &request.to_string()) {
            lock(&self.dialogs).waiting.remove(&id);
        }
        DialogReply {
            answer,
            cancel: dialog.cancel_button,
        }
    }

    /// Asks a question with Cancel and OK buttons over one window's page,
    /// resolving to whether OK was picked.
    pub fn confirm(
        &self,
        window: WindowId,
        title: &str,
        message: &str,
    ) -> impl Future<Output = bool> + use<> {
        let reply = self.show_message(
            window,
            &MessageDialog::new(title)
                .message(message)
                .buttons(["Cancel", "OK"]),
        );
        async move { reply.await.button == 1 }
    }

    pub(crate) fn answer_dialog(&self, line: &str) {
        let mut parts = line.splitn(3, '\t').skip(1);
        let (Some(id), Some(answer)) = (parts.next(), parts.next()) else {
            return;
        };
        let (Ok(id), Ok(answer)) = (id.parse::<u64>(), serde_json::from_str(answer)) else {
            return;
        };
        if let Some((_, sender)) = lock(&self.dialogs).waiting.remove(&id) {
            let _ = sender.send(answer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_default_to_the_last_and_cancel_with_the_first() {
        let dialog = MessageDialog::new("Discard draft?").buttons(["Keep", "Save", "Discard"]);
        assert_eq!((dialog.default_button, dialog.cancel_button), (2, 0));
    }

    #[test]
    fn a_dialog_no_window_shows_answers_with_its_cancel_button() {
        let emitter = BridgeEventEmitter::default();
        let reply = emitter.show_message(7, &MessageDialog::new("Gone").buttons(["No", "Yes"]));
        let answer = crate::async_runtime().block_on(reply);
        assert_eq!(answer.button, 0);
    }
}
