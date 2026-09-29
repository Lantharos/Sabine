use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum, delegate_noop, event_created_child,
    globals::GlobalListContents,
    protocol::{
        wl_data_device::{self, WlDataDevice},
        wl_data_device_manager::WlDataDeviceManager,
        wl_data_offer::{self, WlDataOffer},
        wl_data_source::{self, WlDataSource},
        wl_keyboard::{self, WlKeyboard},
        wl_registry::WlRegistry,
        wl_seat::{self, WlSeat},
    },
};
use wayland_protocols::{
    ext::data_control::v1::client::{
        ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
        ext_data_control_manager_v1::ExtDataControlManagerV1,
        ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
        ext_data_control_source_v1::{self, ExtDataControlSourceV1},
    },
    wp::primary_selection::zv1::client::{
        zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
        zwp_primary_selection_device_v1::{self, ZwpPrimarySelectionDeviceV1},
        zwp_primary_selection_offer_v1::{self, ZwpPrimarySelectionOfferV1},
        zwp_primary_selection_source_v1::{self, ZwpPrimarySelectionSourceV1},
    },
};

use super::objects::{Offer, OfferTypes};
use super::state::{Devices, State};
use crate::clipboard::pipe::send;
use crate::clipboard::{ClipboardContent, Selection};

delegate_noop!(State: WlDataDeviceManager);
delegate_noop!(State: ZwpPrimarySelectionDeviceManagerV1);
delegate_noop!(State: ExtDataControlManagerV1);

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        else {
            return;
        };
        if !matches!(state.devices, Devices::Seat { .. }) {
            return;
        }
        let has_keyboard = capabilities.contains(wl_seat::Capability::Keyboard);
        match (&state.keyboard, has_keyboard) {
            (None, true) => state.keyboard = Some(seat.get_keyboard(handle, ())),
            (Some(keyboard), false) => {
                keyboard.release();
                state.keyboard = None;
            }
            _ => {}
        }
    }
}

impl Dispatch<WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { serial, .. } | wl_keyboard::Event::Key { serial, .. } => {
                state.serial = serial;
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::Selection { id } => {
                state.set_offer(Selection::Clipboard, id.map(Offer::Data));
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id: Some(offer),
            } => state.drag_enter(serial, &surface, x, y, offer),
            wl_data_device::Event::Motion { x, y, .. } => state.drag_motion(x, y),
            wl_data_device::Event::Drop => state.drag_drop(),
            wl_data_device::Event::Leave => state.drag_leave(),
            _ => {}
        }
    }

    event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, OfferTypes::default()),
    ]);
}

impl Dispatch<WlDataOffer, OfferTypes> for State {
    fn event(
        state: &mut Self,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        types: &OfferTypes,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_offer::Event::Offer { mime_type } => types.add(mime_type),
            wl_data_offer::Event::Action {
                dnd_action: WEnum::Value(action),
            } => state.drag_action(offer, action),
            _ => {}
        }
    }
}

impl Dispatch<WlDataSource, ClipboardContent> for State {
    fn event(
        state: &mut Self,
        source: &WlDataSource,
        event: wl_data_source::Event,
        content: &ClipboardContent,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                if let Some(bytes) = content.bytes_for(&mime_type) {
                    send(fd, bytes);
                }
            }
            wl_data_source::Event::Cancelled => state.release_source(source),
            _ => {}
        }
    }
}

impl Dispatch<ZwpPrimarySelectionDeviceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwpPrimarySelectionDeviceV1,
        event: zwp_primary_selection_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_primary_selection_device_v1::Event::Selection { id } = event {
            state.set_offer(Selection::Primary, id.map(Offer::Primary));
        }
    }

    event_created_child!(State, ZwpPrimarySelectionDeviceV1, [
        zwp_primary_selection_device_v1::EVT_DATA_OFFER_OPCODE => (ZwpPrimarySelectionOfferV1, OfferTypes::default()),
    ]);
}

impl Dispatch<ZwpPrimarySelectionOfferV1, OfferTypes> for State {
    fn event(
        _: &mut Self,
        _: &ZwpPrimarySelectionOfferV1,
        event: zwp_primary_selection_offer_v1::Event,
        types: &OfferTypes,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_primary_selection_offer_v1::Event::Offer { mime_type } = event {
            types.add(mime_type);
        }
    }
}

impl Dispatch<ZwpPrimarySelectionSourceV1, ClipboardContent> for State {
    fn event(
        state: &mut Self,
        source: &ZwpPrimarySelectionSourceV1,
        event: zwp_primary_selection_source_v1::Event,
        content: &ClipboardContent,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_primary_selection_source_v1::Event::Send { mime_type, fd } => {
                if let Some(bytes) = content.bytes_for(&mime_type) {
                    send(fd, bytes);
                }
            }
            zwp_primary_selection_source_v1::Event::Cancelled => state.release_source(source),
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlDeviceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtDataControlDeviceV1,
        event: ext_data_control_device_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_device_v1::Event::Selection { id } => {
                state.set_offer(Selection::Clipboard, id.map(Offer::Control));
            }
            ext_data_control_device_v1::Event::PrimarySelection { id } => {
                state.set_offer(Selection::Primary, id.map(Offer::Control));
            }
            _ => {}
        }
    }

    event_created_child!(State, ExtDataControlDeviceV1, [
        ext_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, OfferTypes::default()),
    ]);
}

impl Dispatch<ExtDataControlOfferV1, OfferTypes> for State {
    fn event(
        _: &mut Self,
        _: &ExtDataControlOfferV1,
        event: ext_data_control_offer_v1::Event,
        types: &OfferTypes,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_data_control_offer_v1::Event::Offer { mime_type } = event {
            types.add(mime_type);
        }
    }
}

impl Dispatch<ExtDataControlSourceV1, ClipboardContent> for State {
    fn event(
        state: &mut Self,
        source: &ExtDataControlSourceV1,
        event: ext_data_control_source_v1::Event,
        content: &ClipboardContent,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_data_control_source_v1::Event::Send { mime_type, fd } => {
                if let Some(bytes) = content.bytes_for(&mime_type) {
                    send(fd, bytes);
                }
            }
            ext_data_control_source_v1::Event::Cancelled => state.release_source(source),
            _ => {}
        }
    }
}
