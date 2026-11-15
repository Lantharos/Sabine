//! The compositor's `ext-idle-notify-v1`, on a Wayland connection of the
//! app's own that every watch shares, with a thread reading its events.
//! Watches ask for input idleness where the compositor offers it, so a
//! window that keeps the screen on doesn't keep the person active.

use std::sync::OnceLock;
use std::time::Duration;

use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry::WlRegistry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, delegate_noop};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};

use super::super::{IdleEvents, IdleState};

const INPUT_IDLE_VERSION: u32 = 2;

struct Notifications;

struct Compositor {
    connection: Connection,
    notifier: ExtIdleNotifierV1,
    seat: WlSeat,
    queue: QueueHandle<Notifications>,
}

pub(crate) struct Watch {
    connection: Connection,
    notification: ExtIdleNotificationV1,
}

impl Watch {
    /// Fails when the compositor doesn't offer idle notifications.
    pub(crate) fn start(threshold: Duration, events: IdleEvents) -> Result<Self, String> {
        let compositor = compositor().as_ref().map_err(Clone::clone)?;
        let timeout = u32::try_from(threshold.as_millis()).unwrap_or(u32::MAX);
        let notification = if compositor.notifier.version() >= INPUT_IDLE_VERSION {
            compositor.notifier.get_input_idle_notification(
                timeout,
                &compositor.seat,
                &compositor.queue,
                events,
            )
        } else {
            compositor.notifier.get_idle_notification(
                timeout,
                &compositor.seat,
                &compositor.queue,
                events,
            )
        };
        compositor
            .connection
            .flush()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            connection: compositor.connection.clone(),
            notification,
        })
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.notification.destroy();
        let _ = self.connection.flush();
    }
}

fn compositor() -> &'static Result<Compositor, String> {
    static COMPOSITOR: OnceLock<Result<Compositor, String>> = OnceLock::new();
    COMPOSITOR.get_or_init(|| connect().map_err(|error| error.to_string()))
}

fn connect() -> Result<Compositor, Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<Notifications>(&connection)?;
    let handle = queue.handle();
    let notifier = globals.bind::<ExtIdleNotifierV1, _, _>(&handle, 1..=INPUT_IDLE_VERSION, ())?;
    let seat = globals.bind::<WlSeat, _, _>(&handle, 1..=1, ())?;
    std::thread::Builder::new()
        .name("sabine-idle".into())
        .spawn(move || while queue.blocking_dispatch(&mut Notifications).is_ok() {})?;
    Ok(Compositor {
        connection,
        notifier,
        seat,
        queue: handle,
    })
}

impl Dispatch<ExtIdleNotificationV1, IdleEvents> for Notifications {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        events: &IdleEvents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => events(IdleState::Idle),
            ext_idle_notification_v1::Event::Resumed => events(IdleState::Active),
            _ => {}
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Notifications {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: wayland_client::protocol::wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Notifications: ignore WlSeat);
delegate_noop!(Notifications: ExtIdleNotifierV1);
