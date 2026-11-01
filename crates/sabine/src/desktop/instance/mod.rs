//! One running instance per app: the first launch listens for activations,
//! and later launches hand their arguments to it and exit.

#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;

use std::{
    env,
    io::{self, Read, Write},
    net::Shutdown,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use sabine_platform::{PlatformEvent, SingleInstanceActivation, SingleInstancePolicy};
use serde_json::Value;

use super::{EventQueue, sanitize_id};
use crate::{SabineError, SabineResult};
#[cfg(target_os = "linux")]
pub(crate) use platform::startup_activation_token;
use platform::{Listener, Owner, Peer, Stream};

const IO_TIMEOUT: Duration = Duration::from_secs(2);
const STARTUP_TIMEOUT: Duration = Duration::from_millis(750);
const RETRY_INTERVAL: Duration = Duration::from_millis(25);
const MAX_ACTIVATION_BYTES: u64 = 1024 * 1024;
const WORKERS: usize = 4;
const MAX_PENDING_CONNECTIONS: usize = 32;

/// Who holds an app's instance: this process, listening for activations, or
/// a running one to hand this launch to.
enum Claim {
    Owner(Owner, Listener),
    Running(Peer),
}

pub(super) struct SingleInstanceGuard {
    owner: Owner,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SingleInstanceGuard {
    /// Claims `id` for this process, or hands this launch's arguments to the
    /// instance that already holds it.
    pub(super) fn acquire(
        id: &str,
        policy: SingleInstancePolicy,
        events: EventQueue,
    ) -> SabineResult<Self> {
        let failed = |error: io::Error| SabineError::CreationFailed {
            message: format!("could not check for a running instance: {error}"),
        };
        match platform::claim(&sanitize_id(id)).map_err(failed)? {
            Claim::Owner(owner, listener) => {
                let running = Arc::new(AtomicBool::new(true));
                let thread = serve(listener, policy, events, Arc::clone(&running));
                Ok(Self {
                    owner,
                    running,
                    thread: Some(thread),
                })
            }
            Claim::Running(peer) => {
                notify(&peer).map_err(failed)?;
                Err(SabineError::InstanceAlreadyRunning)
            }
        }
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.owner.wake();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(
    listener: Listener,
    policy: SingleInstancePolicy,
    events: EventQueue,
    running: Arc<AtomicBool>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let (connections, pending) = crossbeam_channel::bounded::<Stream>(MAX_PENDING_CONNECTIONS);
        let workers = (0..WORKERS)
            .map(|_| {
                let pending = pending.clone();
                let events = events.clone();
                let gate = listener.gate();
                thread::spawn(move || {
                    for stream in pending {
                        if let Some(activation) = receive(policy, &gate, stream) {
                            let _ = events.send(PlatformEvent::SingleInstance(activation));
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        while let Ok(stream) = listener.accept() {
            if !running.load(Ordering::Relaxed) || connections.send(stream).is_err() {
                break;
            }
        }
        drop(connections);
        for worker in workers {
            let _ = worker.join();
        }
    })
}

fn receive(
    policy: SingleInstancePolicy,
    gate: &platform::Gate,
    mut stream: Stream,
) -> Option<SingleInstanceActivation> {
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    let mut body = Vec::new();
    Read::by_ref(&mut stream)
        .take(MAX_ACTIVATION_BYTES + 1)
        .read_to_end(&mut body)
        .ok()?;
    if body.len() as u64 > MAX_ACTIVATION_BYTES {
        return None;
    }
    let value: Value = serde_json::from_slice(&body).ok()?;
    if !gate.admits(&stream, &value) {
        return None;
    }
    activation(policy, &value)
}

fn activation(policy: SingleInstancePolicy, value: &Value) -> Option<SingleInstanceActivation> {
    let arguments = value
        .get("arguments")?
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    let mut activation = SingleInstanceActivation::new(policy, arguments);
    if let Some(cwd) = value.get("cwd").and_then(Value::as_str) {
        activation = activation.working_directory(cwd);
    }
    if let Some(token) = value
        .get("activationToken")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        activation = activation.activation_token(token);
    }
    Some(activation)
}

/// Hands this launch to the running instance, retrying while it is still
/// starting to listen.
fn notify(peer: &Peer) -> io::Result<()> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        match send(peer) {
            Ok(()) => return Ok(()),
            Err(_) if Instant::now() < deadline => thread::sleep(RETRY_INTERVAL),
            Err(error) => return Err(error),
        }
    }
}

fn send(peer: &Peer) -> io::Result<()> {
    let mut activation = serde_json::json!({
        "arguments": env::args().collect::<Vec<_>>(),
        "cwd": env::current_dir().ok().map(|path| path.display().to_string()),
    });
    let mut stream = platform::connect(peer, &mut activation)?;
    let body = serde_json::to_vec(&activation).map_err(io::Error::other)?;
    if body.len() as u64 > MAX_ACTIVATION_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "single-instance activation is too large",
        ));
    }
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.write_all(&body)?;
    stream.shutdown(Shutdown::Write)
}
