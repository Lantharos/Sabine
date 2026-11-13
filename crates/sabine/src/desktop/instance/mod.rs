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
    io::{self, Read},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use sabine_platform::{PlatformEvent, SingleInstanceActivation, SingleInstancePolicy};
use serde_json::Value;

use super::{EventQueue, sanitize_id};
use crate::{SabineError, SabineProcessHandle, SabineResult};
#[cfg(target_os = "linux")]
pub(crate) use platform::startup_activation_token;
use platform::{Listener, Owner, Peer, Stream};

const IO_TIMEOUT: Duration = Duration::from_secs(2);
const STARTUP_TIMEOUT: Duration = Duration::from_millis(750);
const RETRY_INTERVAL: Duration = Duration::from_millis(25);
const MAX_ACTIVATION_BYTES: u64 = 1024 * 1024;
const WORKERS: usize = 4;
const MAX_PENDING_CONNECTIONS: usize = 32;
/// How long a launch of the app's newly installed copy waits for the running
/// instance to finish what it is doing and end once it stepped down.
#[cfg(unix)]
const STEP_DOWN_TIMEOUT: Duration = Duration::from_secs(60);

/// The running app the instance answers for, once its process has started.
type Process = Arc<OnceLock<SabineProcessHandle>>;

/// Who holds an app's instance: this process, listening for activations, or
/// a running one to hand this launch to.
enum Claim {
    Owner(Owner, Listener),
    Running(Peer),
}

pub(super) struct SingleInstanceGuard {
    owner: Owner,
    running: Arc<AtomicBool>,
    process: Process,
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
        let id = sanitize_id(id);
        match platform::claim(&id).map_err(failed)? {
            Claim::Owner(owner, listener) => Ok(Self::serve(owner, listener, policy, events)),
            Claim::Running(peer) => {
                if notify(&peer).map_err(failed)? {
                    #[cfg(unix)]
                    if let Claim::Owner(owner, listener) =
                        platform::take_over(&id).map_err(failed)?
                    {
                        return Ok(Self::serve(owner, listener, policy, events));
                    }
                }
                Err(SabineError::InstanceAlreadyRunning)
            }
        }
    }

    fn serve(
        owner: Owner,
        listener: Listener,
        policy: SingleInstancePolicy,
        events: EventQueue,
    ) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let process = Process::default();
        let thread = serve(
            listener,
            policy,
            events,
            Arc::clone(&running),
            Arc::clone(&process),
        );
        Self {
            owner,
            running,
            process,
            thread: Some(thread),
        }
    }

    /// Lets the instance answer for the app's process from now on.
    pub(super) fn attach(&self, handle: &SabineProcessHandle) {
        let _ = self.process.set(handle.clone());
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
    process: Process,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let (connections, pending) = crossbeam_channel::bounded::<Stream>(MAX_PENDING_CONNECTIONS);
        let workers = (0..WORKERS)
            .map(|_| {
                let pending = pending.clone();
                let events = events.clone();
                let gate = listener.gate();
                let process = Arc::clone(&process);
                thread::spawn(move || {
                    for stream in pending {
                        if let Some(activation) = receive(policy, &gate, &process, stream) {
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
    process: &Process,
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
    #[cfg(target_os = "linux")]
    if steps_down(process, &value) {
        let _ = io::Write::write_all(&mut stream, sabine_runtime::instance::STEPPING_DOWN);
        return None;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = process;
    activation(policy, &value)
}

/// Whether the app steps down for the copy of it installed while it ran, as
/// a launch of that copy or its installer asks.
#[cfg(target_os = "linux")]
fn steps_down(process: &Process, message: &Value) -> bool {
    use sabine_runtime::instance::{SUCCESSOR, UPDATED};
    let launch = message.get(SUCCESSOR).and_then(Value::as_bool) == Some(true);
    (launch || message.get(UPDATED).is_some())
        && process
            .get()
            .is_some_and(|process| process.step_down(launch))
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
/// starting to listen. Answers whether the instance stepped down for this
/// launch to take over.
fn notify(peer: &Peer) -> io::Result<bool> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    loop {
        match send(peer) {
            Ok(stepping_down) => return Ok(stepping_down),
            Err(_) if Instant::now() < deadline => thread::sleep(RETRY_INTERVAL),
            Err(error) => return Err(error),
        }
    }
}

fn send(peer: &Peer) -> io::Result<bool> {
    let mut activation = serde_json::json!({
        "arguments": env::args().collect::<Vec<_>>(),
        "cwd": env::current_dir().ok().map(|path| path.display().to_string()),
    });
    let stream = platform::connect(peer, &mut activation)?;
    let body = serde_json::to_vec(&activation).map_err(io::Error::other)?;
    if body.len() as u64 > MAX_ACTIVATION_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "single-instance activation is too large",
        ));
    }
    deliver(stream, &body)
}

#[cfg(unix)]
fn deliver(stream: Stream, body: &[u8]) -> io::Result<bool> {
    sabine_runtime::instance::deliver(stream, body, 2 * IO_TIMEOUT)
}

#[cfg(target_os = "windows")]
fn deliver(mut stream: Stream, body: &[u8]) -> io::Result<bool> {
    use std::io::Write;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream.write_all(body)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    Ok(false)
}
