//! Linux shows notifications through `org.freedesktop.Notifications` when a
//! notification server runs, and otherwise through the XDG desktop portal,
//! which desktops such as GNOME route to their own notification service.

mod freedesktop;
mod portal;

use std::{
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
};

use ashpd::zbus::{Connection, fdo::DBusProxy};
use futures_util::future::{AbortHandle, Abortable};

use super::{Notification, NotificationEvent, NotificationEvents};
use freedesktop::Freedesktop;
use portal::Portal;

enum Command {
    Show(Notification),
    Close(String),
}

enum Service {
    Freedesktop(Freedesktop),
    Portal(Portal),
}

pub(super) struct Backend {
    commands: Option<mpsc::Sender<Command>>,
    listener: AbortHandle,
    worker: Option<JoinHandle<()>>,
}

impl Backend {
    pub(super) fn new(app_id: &str, app_name: &str, events: NotificationEvents) -> Self {
        let (commands, queue) = mpsc::channel();
        let (listener, registration) = AbortHandle::new_pair();
        let app_id = app_id.to_string();
        let app_name = app_name.to_string();
        let worker = thread::spawn(move || {
            let service = pollster::block_on(Service::connect(&app_id)).map(Arc::new);
            let listener = service.as_ref().ok().map(|service| {
                let service = Arc::clone(service);
                let events = Arc::clone(&events);
                let listen =
                    Abortable::new(async move { service.listen(&events).await }, registration);
                thread::spawn(move || {
                    let _ = pollster::block_on(listen);
                })
            });
            for command in queue {
                match (&service, command) {
                    (Ok(service), Command::Show(notification)) => {
                        let id = notification.id.clone();
                        if let Err(message) =
                            pollster::block_on(service.show(&app_id, &app_name, notification))
                        {
                            events(NotificationEvent::Failed { id, message });
                        }
                    }
                    (Ok(service), Command::Close(id)) => {
                        pollster::block_on(service.close(&id, &events));
                    }
                    (Err(message), Command::Show(notification)) => {
                        events(NotificationEvent::Failed {
                            id: notification.id,
                            message: message.clone(),
                        });
                    }
                    (Err(_), Command::Close(_)) => {}
                }
            }
            if let Some(listener) = listener {
                let _ = listener.join();
            }
        });
        Self {
            commands: Some(commands),
            listener,
            worker: Some(worker),
        }
    }

    pub(super) fn show(&self, notification: Notification) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(Command::Show(notification));
        }
    }

    pub(super) fn close(&self, id: &str) {
        if let Some(commands) = &self.commands {
            let _ = commands.send(Command::Close(id.to_string()));
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.commands = None;
        self.listener.abort();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Service {
    /// Only a running notification server counts, so a desktop that merely
    /// lists one as activatable does not start it.
    async fn connect(app_id: &str) -> Result<Self, String> {
        let connection = Connection::session()
            .await
            .map_err(|error| error.to_string())?;
        let running = DBusProxy::new(&connection)
            .await
            .map_err(|error| error.to_string())?
            .name_has_owner(
                freedesktop::SERVICE
                    .try_into()
                    .map_err(|error: ashpd::zbus::names::Error| error.to_string())?,
            )
            .await
            .unwrap_or(false);
        if running {
            Freedesktop::connect(&connection)
                .await
                .map(Self::Freedesktop)
                .map_err(|error| error.to_string())
        } else {
            Portal::connect(app_id)
                .await
                .map(Self::Portal)
                .map_err(|error| error.to_string())
        }
    }

    async fn show(
        &self,
        app_id: &str,
        app_name: &str,
        notification: Notification,
    ) -> Result<(), String> {
        match self {
            Self::Freedesktop(service) => service.show(app_id, app_name, notification).await,
            Self::Portal(service) => service.show(notification).await,
        }
    }

    async fn close(&self, id: &str, events: &NotificationEvents) {
        match self {
            Self::Freedesktop(service) => service.close(id).await,
            Self::Portal(service) => service.close(id, events).await,
        }
    }

    async fn listen(&self, events: &NotificationEvents) {
        match self {
            Self::Freedesktop(service) => service.listen(events).await,
            Self::Portal(service) => service.listen(events).await,
        }
    }
}
