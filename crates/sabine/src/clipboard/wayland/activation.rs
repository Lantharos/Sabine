use std::sync::Mutex;

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_seat::WlSeat, wl_surface::WlSurface},
};
use wayland_protocols::xdg::activation::v1::client::{
    xdg_activation_token_v1::{self, XdgActivationTokenV1},
    xdg_activation_v1::XdgActivationV1,
};

use super::state::State;
use crate::clipboard::TokenReply;

delegate_noop!(State: XdgActivationV1);

/// Waits for the compositor to name the token it made.
pub(super) struct PendingToken(Mutex<Option<TokenReply>>);

impl PendingToken {
    fn answer(&self, token: Option<String>) {
        if let Some(reply) = self.0.lock().unwrap().take() {
            reply(token);
        }
    }
}

/// Asks for a token that lets another app's window take focus, made for the
/// window while it has the keyboard so the compositor can tell the person
/// is using it.
pub(super) fn request(
    activation: Option<&XdgActivationV1>,
    surface: Option<WlSurface>,
    focus: Option<(&WlSeat, u32)>,
    handle: &QueueHandle<State>,
    reply: TokenReply,
) {
    let Some(activation) = activation else {
        reply(None);
        return;
    };
    let token = activation.get_activation_token(handle, PendingToken(Mutex::new(Some(reply))));
    if let Some((seat, serial)) = focus {
        token.set_serial(serial, seat);
    }
    if let Some(surface) = surface {
        token.set_surface(&surface);
    }
    token.commit();
}

impl Dispatch<XdgActivationTokenV1, PendingToken> for State {
    fn event(
        _: &mut Self,
        proxy: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        pending: &PendingToken,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            pending.answer(Some(token));
            proxy.destroy();
        }
    }
}

impl Drop for PendingToken {
    fn drop(&mut self) {
        self.answer(None);
    }
}
