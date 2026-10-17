//! Calls to the XDG desktop portal, shared by Sabine's notifications,
//! appearance, and global shortcuts.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use futures_util::StreamExt;
use zbus::{Connection, MatchRule, MessageStream, OwnedObjectPath, OwnedVariant, Proxy, Variant};

pub const SERVICE: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
const REGISTRY: &str = "org.freedesktop.host.portal.Registry";
const REQUEST: &str = "org.freedesktop.portal.Request";

/// The named values a portal answers or is given.
pub type Results = HashMap<String, OwnedVariant>;
pub type Options<'a> = HashMap<&'a str, Variant<'a>>;

static TOKENS: AtomicU32 = AtomicU32::new(0);

pub async fn proxy(
    connection: &Connection,
    interface: &'static str,
) -> zbus::Result<Proxy<'static>> {
    Proxy::new(connection, SERVICE, PATH, interface).await
}

/// Tells the portal which desktop entry this unsandboxed process belongs to.
/// A process registers once, so registering again is not an error.
pub async fn register_host_app(connection: &Connection, app_id: &str) -> zbus::Result<()> {
    let registry = proxy(connection, REGISTRY).await?;
    match registry
        .call::<_, _, ()>("Register", &(app_id, Options::new()))
        .await
    {
        Err(error) if already_registered(&error) => Ok(()),
        result => result,
    }
}

fn already_registered(error: &zbus::Error) -> bool {
    let message = error.to_string();
    message.contains("already associated") || message.contains("already registered")
}

/// A token for the `handle_token` of a request or session, unique in this
/// process.
pub fn token() -> String {
    format!(
        "sabine{}_{}",
        std::process::id(),
        TOKENS.fetch_add(1, Ordering::Relaxed)
    )
}

/// Calls a portal method that answers through a `Request` object, and waits
/// for that answer. `call` receives the `handle_token` to pass in its options.
/// A request the person cancelled or the portal turned down is an error.
pub async fn request<F, Fut>(connection: &Connection, call: F) -> zbus::Result<Results>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = zbus::Result<OwnedObjectPath>>,
{
    let token = token();
    let sender = connection
        .unique_name()
        .ok_or_else(|| zbus::Error::Failure("not connected to the session bus".into()))?
        .trim_start_matches(':')
        .replace('.', "_");
    let path = format!("{PATH}/request/{sender}/{token}");
    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(REQUEST)
        .member("Response")
        .path(path.as_str())
        .build()?;
    let mut responses = MessageStream::for_match_rule(rule, connection, None).await?;
    call(token).await?;
    let message = responses
        .next()
        .await
        .ok_or_else(|| zbus::Error::Failure("the portal closed the request".into()))??;
    let (response, results): (u32, Results) = message.body().deserialize()?;
    match response {
        0 => Ok(results),
        1 => Err(zbus::Error::Failure("the request was cancelled".into())),
        _ => Err(zbus::Error::Failure(
            "the portal turned the request down".into(),
        )),
    }
}
