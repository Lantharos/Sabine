//! The XDG desktop portal's Inhibit interface keeps the session from
//! suspending, and from going idle so the screen stays on, also inside a
//! sandbox. Desktops whose portal has no Inhibit get logind's sleep lock,
//! which keeps the system awake.

use tokio::sync::OnceCell;
use zbus::{Connection, OwnedFd, OwnedObjectPath, Proxy, Variant};

use crate::portal::{self, Options};

const INHIBIT: &str = "org.freedesktop.portal.Inhibit";
const REQUEST: &str = "org.freedesktop.portal.Request";
const SUSPEND: u32 = 4;
const IDLE: u32 = 8;
const LOGIND: &str = "org.freedesktop.login1";
const LOGIND_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";

/// What keeps the machine awake: the portal's request, closed once the
/// blocker goes, or logind's lock, released as it closes.
pub(super) enum Blocker {
    Portal(OwnedObjectPath),
    Logind { _lock: OwnedFd },
}

impl Blocker {
    pub(super) async fn new(display: bool, reason: &str) -> Result<Self, String> {
        match inhibit_session(display, reason).await {
            Ok(request) => Ok(Self::Portal(request)),
            Err(portal) => inhibit_sleep(reason)
                .await
                .map(|lock| Self::Logind { _lock: lock })
                .map_err(|logind| format!("{portal}; {logind}")),
        }
    }
}

impl Drop for Blocker {
    fn drop(&mut self) {
        if let Self::Portal(request) = self {
            let request = request.clone();
            crate::async_runtime().spawn(async move {
                let _ = close(&request).await;
            });
        }
    }
}

async fn session() -> zbus::Result<&'static Connection> {
    static SESSION: OnceCell<Connection> = OnceCell::const_new();
    SESSION.get_or_try_init(Connection::session).await
}

async fn inhibit_session(display: bool, reason: &str) -> zbus::Result<OwnedObjectPath> {
    let inhibit = portal::proxy(session().await?, INHIBIT).await?;
    let flags = if display { SUSPEND | IDLE } else { SUSPEND };
    let options = Options::from([
        ("handle_token", Variant::from(portal::token())),
        ("reason", Variant::from(reason)),
    ]);
    inhibit.call("Inhibit", &("", flags, options)).await
}

async fn close(request: &OwnedObjectPath) -> zbus::Result<()> {
    Proxy::new(session().await?, portal::SERVICE, request, REQUEST)
        .await?
        .call("Close", &())
        .await
}

async fn inhibit_sleep(reason: &str) -> zbus::Result<OwnedFd> {
    let bus = Connection::system().await?;
    let program = std::env::current_exe()
        .ok()
        .and_then(|program| Some(program.file_stem()?.to_string_lossy().into_owned()))
        .unwrap_or_default();
    Proxy::new(&bus, LOGIND, LOGIND_PATH, MANAGER)
        .await?
        .call("Inhibit", &("sleep", program, reason, "block"))
        .await
}
