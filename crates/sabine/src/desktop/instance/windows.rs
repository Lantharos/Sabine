// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// A named mutex per session holds the instance, and a loopback socket takes
// activations. Its port and a random token sit in the user's registry, so
// only launches by the same user, which can read them, get through.

use std::{
    io,
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    time::Duration,
};

use sabine_service::windows_registry::{
    current_user_value, delete_current_user_key, set_current_user_value,
};
use serde_json::Value;
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::{
            RemoteDesktop::ProcessIdToSessionId,
            Threading::{CreateMutexW, GetCurrentProcessId},
        },
        UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow},
    },
    core::HSTRING,
};

use super::Claim;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(250);
const ENDPOINT_VALUE: &str = "Endpoint";

pub(super) type Stream = TcpStream;

/// Holds the instance mutex and the registry key naming the listener.
pub(super) struct Owner {
    mutex: HANDLE,
    endpoint_key: String,
    port: u16,
}

impl Owner {
    /// Wakes the listener so it notices the instance is ending.
    pub(super) fn wake(&self) {
        let _ = TcpStream::connect_timeout(&loopback(self.port), CONNECT_TIMEOUT);
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        let _ = delete_current_user_key(&self.endpoint_key);
        unsafe {
            let _ = CloseHandle(self.mutex);
        }
    }
}

pub(super) struct Listener {
    listener: TcpListener,
    token: String,
}

impl Listener {
    pub(super) fn accept(&self) -> io::Result<Stream> {
        self.listener.accept().map(|(stream, _)| stream)
    }

    pub(super) fn gate(&self) -> Gate {
        Gate(self.token.clone())
    }
}

/// Admits local launches that present the token from the registry.
#[derive(Clone)]
pub(super) struct Gate(String);

impl Gate {
    pub(super) fn admits(&self, stream: &Stream, activation: &Value) -> bool {
        stream.peer_addr().is_ok_and(|peer| peer.ip().is_loopback())
            && activation
                .get("token")
                .and_then(Value::as_str)
                .is_some_and(|token| tokens_match(token, &self.0))
    }
}

pub(super) struct Peer(String);

pub(super) fn claim(id: &str) -> io::Result<Claim> {
    let endpoint_key = format!(
        "Software\\Sabine\\Instances\\{id}-{}",
        current_session_id()?
    );
    let mutex_name = HSTRING::from(format!("Local\\sabine-{id}"));
    let mutex = unsafe { CreateMutexW(None, false, &mutex_name) }.map_err(io::Error::other)?;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            let _ = CloseHandle(mutex);
        }
        return Ok(Claim::Running(Peer(endpoint_key)));
    }
    let mut owner = Owner {
        mutex,
        endpoint_key,
        port: 0,
    };
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    owner.port = listener.local_addr()?.port();
    let token = authentication_token()?;
    let endpoint = serde_json::json!({ "port": owner.port, "token": token }).to_string();
    set_current_user_value(&owner.endpoint_key, ENDPOINT_VALUE, &endpoint)?;
    Ok(Claim::Owner(owner, Listener { listener, token }))
}

/// Connects to the running instance and lets it take the foreground, which
/// Windows grants only to the process the person just launched.
pub(super) fn connect(peer: &Peer, activation: &mut Value) -> io::Result<Stream> {
    let (port, token) = read_endpoint(&peer.0)?;
    activation["token"] = token.into();
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
    TcpStream::connect_timeout(&loopback(port), CONNECT_TIMEOUT)
}

fn read_endpoint(endpoint_key: &str) -> io::Result<(u16, String)> {
    let invalid = |message: &str| io::Error::new(io::ErrorKind::InvalidData, message.to_string());
    let body = current_user_value(endpoint_key, ENDPOINT_VALUE).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "the running instance has not published its endpoint yet",
        )
    })?;
    let value: Value = serde_json::from_str(&body).map_err(io::Error::other)?;
    let port = value
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port != 0)
        .ok_or_else(|| invalid("single-instance endpoint has an invalid port"))?;
    let token = value
        .get("token")
        .and_then(Value::as_str)
        .filter(|token| token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| invalid("single-instance endpoint has an invalid token"))?;
    Ok((port, token.to_string()))
}

fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

fn current_session_id() -> io::Result<u32> {
    let mut session_id = 0_u32;
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session_id) }
        .map_err(io::Error::other)?;
    Ok(session_id)
}

fn authentication_token() -> io::Result<String> {
    sabine_runtime::random_hex(32)
}

fn tokens_match(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}
