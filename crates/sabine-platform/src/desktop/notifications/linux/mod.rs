//! Linux shows notifications through `org.freedesktop.Notifications` when a
//! notification server runs, and otherwise through the XDG desktop portal,
//! which desktops such as GNOME route to their own notification service.

mod freedesktop;
mod portal;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use zbus::{Connection, fdo::DBusProxy, names::BusName};

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

/// Shows and closes notifications in the order they were asked for, while
/// listening for clicks and closes, on Sabine's async runtime.
pub(super) struct Backend {
    commands: mpsc::UnboundedSender<Command>,
    worker: JoinHandle<()>,
}

impl Backend {
    pub(super) fn new(app_id: &str, app_name: &str, events: NotificationEvents) -> Self {
        let (commands, queue) = mpsc::unbounded_channel();
        let app_id = app_id.to_string();
        let app_name = app_name.to_string();
        let worker = crate::async_runtime().spawn(async move {
            match Service::connect(&app_id).await {
                Ok(service) => service.run(&app_id, &app_name, queue, &events).await,
                Err(message) => refuse(queue, &events, &message).await,
            }
        });
        Self { commands, worker }
    }

    pub(super) fn show(&self, notification: Notification) {
        let _ = self.commands.send(Command::Show(notification));
    }

    pub(super) fn close(&self, id: &str) {
        let _ = self.commands.send(Command::Close(id.to_string()));
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

async fn refuse(
    mut queue: mpsc::UnboundedReceiver<Command>,
    events: &NotificationEvents,
    message: &str,
) {
    while let Some(command) = queue.recv().await {
        if let Command::Show(notification) = command {
            events(NotificationEvent::Failed {
                id: notification.id,
                message: message.to_string(),
            });
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
                BusName::from_static_str(freedesktop::SERVICE)
                    .map_err(|error| error.to_string())?,
            )
            .await
            .unwrap_or(false);
        if running {
            Freedesktop::connect(&connection)
                .await
                .map(Self::Freedesktop)
                .map_err(|error| error.to_string())
        } else {
            Portal::connect(&connection, app_id)
                .await
                .map(Self::Portal)
                .map_err(|error| error.to_string())
        }
    }

    async fn run(
        &self,
        app_id: &str,
        app_name: &str,
        mut queue: mpsc::UnboundedReceiver<Command>,
        events: &NotificationEvents,
    ) {
        let listen = self.listen(events);
        tokio::pin!(listen);
        let mut listening = true;
        loop {
            tokio::select! {
                () = &mut listen, if listening => listening = false,
                command = queue.recv() => match command {
                    Some(command) => self.handle(app_id, app_name, command, events).await,
                    None => break,
                },
            }
        }
    }

    async fn handle(
        &self,
        app_id: &str,
        app_name: &str,
        command: Command,
        events: &NotificationEvents,
    ) {
        match command {
            Command::Show(notification) => {
                let id = notification.id.clone();
                let shown = match self {
                    Self::Freedesktop(service) => {
                        service.show(app_id, app_name, notification).await
                    }
                    Self::Portal(service) => service.show(notification).await,
                };
                if let Err(message) = shown {
                    events(NotificationEvent::Failed { id, message });
                }
            }
            Command::Close(id) => match self {
                Self::Freedesktop(service) => service.close(&id).await,
                Self::Portal(service) => service.close(&id, events).await,
            },
        }
    }

    async fn listen(&self, events: &NotificationEvents) {
        match self {
            Self::Freedesktop(service) => service.listen(events).await,
            Self::Portal(service) => service.listen(events).await,
        }
    }
}
