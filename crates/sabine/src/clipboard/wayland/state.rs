use std::{
    os::{fd::AsFd, unix::net::UnixStream},
    sync::{Arc, atomic::AtomicBool},
    thread,
    time::{Duration, Instant},
};

use crossbeam_channel::Sender;
use wayland_backend::client::ObjectId;
use wayland_client::{
    Connection, EventQueue, QueueHandle,
    globals::registry_queue_init,
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::WlDataDeviceManager,
        wl_keyboard::WlKeyboard, wl_seat::WlSeat,
    },
};
use wayland_protocols::{
    ext::data_control::v1::client::{
        ext_data_control_device_v1::ExtDataControlDeviceV1,
        ext_data_control_manager_v1::ExtDataControlManagerV1,
    },
    wp::primary_selection::zv1::client::{
        zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1,
        zwp_primary_selection_device_v1::ZwpPrimarySelectionDeviceV1,
    },
};

use super::Command;
use super::drag::Drag;
use super::objects::{Offer, Source};
use crate::clipboard::content::read_plan;
use crate::clipboard::pipe::{pipe, read_to_end};
use crate::clipboard::{ClipboardContent, DropEvent, Reply, Selection, Waker};

const READ_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) enum Devices {
    /// `ext-data-control`, which reaches both selections without keyboard
    /// focus and leaves drag and drop to the window toolkit.
    Control {
        manager: ExtDataControlManagerV1,
        device: ExtDataControlDeviceV1,
    },
    /// The seat's data device. Compositors without data control route
    /// selections and drops to a client's newest data device, so this one
    /// also receives the files dropped on the window.
    Seat {
        manager: WlDataDeviceManager,
        device: WlDataDevice,
        primary: Option<(
            ZwpPrimarySelectionDeviceManagerV1,
            ZwpPrimarySelectionDeviceV1,
        )>,
    },
}

pub(super) struct Channels {
    pub(super) commands: Sender<Command>,
    pub(super) wake: UnixStream,
    pub(super) drops: Sender<DropEvent>,
    pub(super) waker: Waker,
    pub(super) outgoing_drag: Arc<AtomicBool>,
}

pub(super) struct State {
    pub(super) connection: Connection,
    pub(super) handle: QueueHandle<State>,
    pub(super) serial: u32,
    pub(super) keyboard: Option<WlKeyboard>,
    pub(super) devices: Devices,
    pub(super) offers: [Option<Offer>; 2],
    owned: [Option<Source>; 2],
    pub(super) drag: Option<Drag>,
    pub(super) next_drag: u64,
    pub(super) surface: Option<ObjectId>,
    pub(super) channels: Channels,
}

impl State {
    pub(super) fn connect(
        connection: Connection,
        channels: Channels,
    ) -> Result<(EventQueue<Self>, Self), String> {
        let (globals, queue) =
            registry_queue_init::<Self>(&connection).map_err(|error| error.to_string())?;
        let handle = queue.handle();
        let seat: WlSeat = globals
            .bind(&handle, 1..=7, ())
            .map_err(|_| "the compositor has no seat".to_string())?;
        let devices = match globals.bind::<ExtDataControlManagerV1, _, _>(&handle, 1..=1, ()) {
            Ok(manager) => {
                let device = manager.get_data_device(&seat, &handle, ());
                Devices::Control { manager, device }
            }
            Err(_) => {
                let manager: WlDataDeviceManager = globals
                    .bind(&handle, 1..=3, ())
                    .map_err(|_| "the compositor has no clipboard".to_string())?;
                let device = manager.get_data_device(&seat, &handle, ());
                let primary = globals
                    .bind::<ZwpPrimarySelectionDeviceManagerV1, _, _>(&handle, 1..=1, ())
                    .ok()
                    .map(|manager| {
                        let device = manager.get_device(&seat, &handle, ());
                        (manager, device)
                    });
                Devices::Seat {
                    manager,
                    device,
                    primary,
                }
            }
        };
        let state = Self {
            connection,
            handle,
            serial: 0,
            keyboard: None,
            devices,
            offers: [None, None],
            owned: [None, None],
            drag: None,
            next_drag: 0,
            surface: None,
            channels,
        };
        Ok((queue, state))
    }

    pub(super) fn owns_drops(&self) -> bool {
        matches!(self.devices, Devices::Seat { .. })
    }

    pub(super) fn handle(&mut self, command: Command) {
        match command {
            Command::Read {
                selection,
                types,
                reply,
            } => self.read(selection, types, reply),
            Command::Write { selection, content } => self.write(selection, content),
            Command::Surface(surface) => self.surface = surface,
            Command::DragData { drag, paths } => self.receive_drag_data(drag, paths),
        }
    }

    pub(super) fn set_offer(&mut self, selection: Selection, offer: Option<Offer>) {
        let slot = &mut self.offers[selection.index()];
        if let Some(previous) = slot.take()
            && Some(&previous) != offer.as_ref()
        {
            previous.destroy();
        }
        *slot = offer;
    }

    pub(super) fn release_source(&mut self, source: &impl wayland_client::Proxy) {
        for owned in &mut self.owned {
            if owned.as_ref().is_some_and(|owned| owned.is(source)) {
                owned.take().expect("checked above").destroy();
            }
        }
    }

    fn read(&self, selection: Selection, types: Option<Vec<String>>, reply: Reply) {
        let Some(offer) = self.offers[selection.index()].clone() else {
            reply(Ok(ClipboardContent::default()));
            return;
        };
        let plan = read_plan(&offer.types(), types.as_deref());
        let connection = self.connection.clone();
        thread::spawn(move || reply(receive(&connection, &offer, plan)));
    }

    fn write(&mut self, selection: Selection, content: ClipboardContent) {
        let handle = &self.handle;
        let source = match (&self.devices, selection) {
            (Devices::Control { manager, device }, _) => {
                let proxy = manager.create_data_source(handle, content.clone());
                Source::Control(proxy.clone()).offer_types(&content);
                match selection {
                    Selection::Clipboard => device.set_selection(Some(&proxy)),
                    Selection::Primary => device.set_primary_selection(Some(&proxy)),
                }
                Source::Control(proxy)
            }
            (
                Devices::Seat {
                    manager, device, ..
                },
                Selection::Clipboard,
            ) => {
                let proxy = manager.create_data_source(handle, content.clone());
                Source::Data(proxy.clone()).offer_types(&content);
                device.set_selection(Some(&proxy), self.serial);
                Source::Data(proxy)
            }
            (
                Devices::Seat {
                    primary: Some((manager, device)),
                    ..
                },
                Selection::Primary,
            ) => {
                let proxy = manager.create_source(handle, content.clone());
                Source::Primary(proxy.clone()).offer_types(&content);
                device.set_selection(Some(&proxy), self.serial);
                Source::Primary(proxy)
            }
            (Devices::Seat { primary: None, .. }, Selection::Primary) => return,
        };
        if let Some(previous) = self.owned[selection.index()].replace(source) {
            previous.destroy();
        }
        let _ = self.connection.flush();
    }
}

pub(super) fn receive(
    connection: &Connection,
    offer: &Offer,
    plan: Vec<(String, String)>,
) -> Result<ClipboardContent, String> {
    let deadline = Instant::now() + READ_TIMEOUT;
    let mut content = ClipboardContent::default();
    for (source, reported) in plan {
        let (reader, writer) = pipe().map_err(|error| error.to_string())?;
        offer.receive(source, writer.as_fd());
        drop(writer);
        connection.flush().map_err(|error| error.to_string())?;
        let bytes = read_to_end(reader, deadline).map_err(|error| error.to_string())?;
        content.push(reported, bytes);
    }
    Ok(content)
}
