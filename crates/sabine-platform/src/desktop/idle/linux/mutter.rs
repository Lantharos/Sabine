//! GNOME's idle monitor, for compositors without `ext-idle-notify-v1`. Each
//! watch has a bus connection of its own, and Mutter forgets the watches of
//! a connection once it closes.

use std::time::Duration;

use futures_util::StreamExt;
use tokio::task::JoinHandle;
use zbus::{Connection, Proxy, proxy::SignalStream};

use super::super::{IdleEvents, IdleState};

const SERVICE: &str = "org.gnome.Mutter.IdleMonitor";
const PATH: &str = "/org/gnome/Mutter/IdleMonitor/Core";

pub(crate) struct Watch {
    task: JoinHandle<()>,
}

impl Watch {
    pub(crate) async fn start(threshold: Duration, events: IdleEvents) -> Result<Self, String> {
        let bus = Connection::session()
            .await
            .map_err(|error| error.to_string())?;
        let monitor = Proxy::new(&bus, SERVICE, PATH, SERVICE)
            .await
            .map_err(|error| error.to_string())?;
        let fired = monitor
            .receive_signal("WatchFired")
            .await
            .map_err(|error| error.to_string())?;
        let interval = u64::try_from(threshold.as_millis()).unwrap_or(u64::MAX);
        let away = monitor
            .call::<_, _, u32>("AddIdleWatch", &(interval,))
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self {
            task: crate::async_runtime().spawn(async move {
                let _ = watch(&monitor, fired, away, &events).await;
            }),
        })
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Mutter fires an idle watch each time the person has been away for its
/// interval, and a watch for activity once, so each return asks for a new
/// one.
async fn watch(
    monitor: &Proxy<'static>,
    mut fired: SignalStream<'static>,
    away: u32,
    events: &IdleEvents,
) -> zbus::Result<()> {
    let mut back = None;
    while let Some(signal) = fired.next().await {
        let Ok(watch) = signal.body().deserialize::<u32>() else {
            continue;
        };
        if watch == away {
            events(IdleState::Idle);
            back = Some(monitor.call::<_, _, u32>("AddUserActiveWatch", &()).await?);
        } else if back == Some(watch) {
            back = None;
            events(IdleState::Active);
        }
    }
    Ok(())
}
