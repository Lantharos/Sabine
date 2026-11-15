//! logind reports sleep and the lock state of the person's graphical
//! session, and UPower the power source, on the system bus.

use futures_util::{StreamExt, stream};
use tokio::task::JoinHandle;
use zbus::{Connection, OwnedObjectPath, Proxy};

use super::{PowerSource, Reporter};

const LOGIND: &str = "org.freedesktop.login1";
const LOGIND_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const USER: &str = "org.freedesktop.login1.User";
/// The user of whoever asks.
const OWN_USER_PATH: &str = "/org/freedesktop/login1/user/self";
const SESSION: &str = "org.freedesktop.login1.Session";
const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
/// The battery UPower combines from every battery the machine has.
const DISPLAY_DEVICE_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DEVICE: &str = "org.freedesktop.UPower.Device";

pub(super) struct Watcher {
    task: JoinHandle<()>,
}

impl Watcher {
    pub(super) fn start(reporter: Reporter) -> Self {
        Self {
            task: crate::async_runtime().spawn(async move {
                let Ok(bus) = Connection::system().await else {
                    return;
                };
                let _ = tokio::join!(
                    watch_sleep(&bus, &reporter),
                    watch_lock(&bus, &reporter),
                    watch_source(&bus, &reporter),
                );
            }),
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn watch_sleep(bus: &Connection, reporter: &Reporter) -> zbus::Result<()> {
    let manager = Proxy::new(bus, LOGIND, LOGIND_PATH, MANAGER).await?;
    let mut signals = manager.receive_signal("PrepareForSleep").await?;
    while let Some(signal) = signals.next().await {
        match signal.body().deserialize::<bool>() {
            Ok(true) => reporter.suspending(),
            Ok(false) => reporter.resumed(),
            Err(_) => {}
        }
    }
    Ok(())
}

/// Screen lockers set the `LockedHint` of the session they lock, which is
/// the session the person's user shows on its display.
async fn watch_lock(bus: &Connection, reporter: &Reporter) -> zbus::Result<()> {
    let user = Proxy::new(bus, LOGIND, OWN_USER_PATH, USER).await?;
    let (_, session): (String, OwnedObjectPath) = user.get_property("Display").await?;
    if session.as_str() == "/" {
        return Ok(());
    }
    let session = Proxy::new(bus, LOGIND, session, SESSION).await?;
    let mut changes = session.receive_property_changed::<bool>("LockedHint").await;
    reporter.locked(session.get_property("LockedHint").await?);
    while let Some(change) = changes.next().await {
        if let Ok(locked) = change.get().await {
            reporter.locked(locked);
        }
    }
    Ok(())
}

async fn watch_source(bus: &Connection, reporter: &Reporter) -> zbus::Result<()> {
    let upower = Proxy::new(bus, UPOWER, UPOWER_PATH, UPOWER).await?;
    let battery = Proxy::new(bus, UPOWER, DISPLAY_DEVICE_PATH, DEVICE).await?;
    let mut changes = stream::select_all([
        upower
            .receive_property_changed::<bool>("OnBattery")
            .await
            .map(drop)
            .boxed(),
        battery
            .receive_property_changed::<bool>("IsPresent")
            .await
            .map(drop)
            .boxed(),
        battery
            .receive_property_changed::<f64>("Percentage")
            .await
            .map(drop)
            .boxed(),
    ]);
    loop {
        let present = battery.get_property::<bool>("IsPresent").await?;
        reporter.source(PowerSource {
            on_battery: upower.get_property("OnBattery").await?,
            battery_level: if present {
                Some(battery.get_property::<f64>("Percentage").await?.round() as u8)
            } else {
                None
            },
        });
        if changes.next().await.is_none() {
            return Ok(());
        }
    }
}
