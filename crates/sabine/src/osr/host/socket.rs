// ☢️ WARNING: RADIOACTIVE WINDOWS SLOP BELOW ☢️
//
// Winsock accepts inherit the listener's nonblocking mode. The listener must stay
// blocking so the accepted OSR stream is blocking for authentication and frame
// reads, or WSAEWOULDBLOCK (10035) turns a healthy connection into a browser crash loop.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use winit::event_loop::EventLoopProxy;

use crate::osr::control::ControlWriter;
use crate::osr::message_queue::MessageQueue;
#[cfg(unix)]
use crate::osr::protocol::PaintSlots;
use crate::osr::protocol::WireReader;
use crate::osr::transport::{IpcEndpoint, IpcListener, IpcStream};

use super::types::OsrHostEvent;

pub(super) struct SocketReader {
    state: Arc<ReaderState>,
    endpoint: IpcEndpoint,
}

struct ReaderState {
    stopped: AtomicBool,
    stream: Mutex<Option<IpcStream>>,
    messages: Arc<MessageQueue>,
}

impl Drop for SocketReader {
    fn drop(&mut self) {
        self.state.stopped.store(true, Ordering::Release);
        self.state.messages.close();
        if let Ok(stream) = self.state.stream.lock()
            && let Some(stream) = stream.as_ref()
        {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        self.endpoint.wake_listener();
        self.endpoint.unlink();
    }
}

impl ReaderState {
    fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    fn send(&self, sender: &mpsc::SyncSender<OsrHostEvent>, mut event: OsrHostEvent) -> bool {
        while !self.stopped() {
            match sender.try_send(event) {
                Ok(()) => return true,
                Err(mpsc::TrySendError::Disconnected(_)) => return false,
                Err(mpsc::TrySendError::Full(pending)) => {
                    event = pending;
                    thread::sleep(Duration::from_millis(10));
                }
            }
        }
        false
    }
}

pub(super) fn start_socket_reader(
    generation: u64,
    listener: IpcListener,
    endpoint: IpcEndpoint,
    authentication_token: String,
    sender: mpsc::SyncSender<OsrHostEvent>,
    proxy: EventLoopProxy,
    #[cfg(target_os = "macos")] surfaces: Arc<crate::osr::accel::SurfaceRegistry>,
) -> SocketReader {
    let state = Arc::new(ReaderState {
        stopped: AtomicBool::new(false),
        stream: Mutex::new(None),
        messages: Arc::new(MessageQueue::new()),
    });
    let reader = SocketReader {
        state: Arc::clone(&state),
        endpoint: endpoint.clone(),
    };
    thread::spawn(move || {
        let messages = Arc::clone(&state.messages);
        let stream = loop {
            if state.stopped() {
                return;
            }
            let mut candidate = match listener.accept() {
                Ok((candidate, _)) => candidate,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    endpoint.unlink();
                    state.send(&sender, OsrHostEvent::Disconnected(generation));
                    proxy.wake_up();
                    return;
                }
            };
            if state.stopped() {
                return;
            }
            if let Err(error) = candidate.set_read_timeout(Some(Duration::from_millis(750))) {
                eprintln!("Sabine OSR could not configure authentication socket: {error}");
                continue;
            }
            match crate::osr::transport::authenticate(&mut candidate, &authentication_token) {
                Ok(crate::osr::transport::Authentication::Accepted) => {
                    if let Err(error) = candidate.set_read_timeout(None) {
                        eprintln!("Sabine OSR could not clear authentication deadline: {error}");
                        continue;
                    }
                    break candidate;
                }
                Ok(crate::osr::transport::Authentication::Probe) => continue,
                Err(error) => {
                    eprintln!("Sabine OSR reject connect: {error}");
                }
            }
        };
        let connection = stream.try_clone().and_then(|writer| {
            let control = Arc::new(ControlWriter::start(writer.try_clone()?)?);
            Ok((writer, control, stream.try_clone()?))
        });
        let (writer, control, owned) = match connection {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("Sabine OSR could not set up the browser connection: {error}");
                let _ = stream.shutdown(std::net::Shutdown::Both);
                endpoint.unlink();
                state.send(&sender, OsrHostEvent::Disconnected(generation));
                proxy.wake_up();
                return;
            }
        };
        if let Ok(mut current) = state.stream.lock() {
            *current = Some(owned);
        }
        #[cfg(unix)]
        let mut wire = WireReader::new(
            stream,
            PaintSlots::new(Arc::clone(&control)),
            #[cfg(target_os = "macos")]
            surfaces,
        );
        #[cfg(not(unix))]
        let mut wire = WireReader::new(stream);
        match wire.read_host_protocol() {
            Ok(Some(version)) if version == sabine_host::HOST_PROTOCOL_VERSION => {}
            Ok(None) => {
                endpoint.unlink();
                state.send(&sender, OsrHostEvent::Disconnected(generation));
                proxy.wake_up();
                return;
            }
            Ok(Some(_)) | Err(_) => {
                endpoint.unlink();
                state.send(&sender, OsrHostEvent::IncompatibleHost(generation));
                proxy.wake_up();
                return;
            }
        }
        if !state.send(
            &sender,
            OsrHostEvent::Connected(generation, writer, Arc::clone(&control)),
        ) {
            return;
        }
        proxy.wake_up();
        while !state.stopped() {
            match wire.read() {
                Ok(Some(message)) => {
                    let queued = messages.push(message);
                    if let Some(slot) = queued.replaced_slot {
                        let _ = control.send(format!("accel_release\t{slot}\n"));
                    }
                    if queued.wake {
                        if !state.send(
                            &sender,
                            OsrHostEvent::MessagesReady(generation, Arc::clone(&messages)),
                        ) {
                            break;
                        }
                        proxy.wake_up();
                    }
                }
                Ok(None) => break,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
                    ) =>
                {
                    break;
                }
                Err(error) => {
                    eprintln!("Sabine OSR socket read failed: {error}");
                    break;
                }
            }
        }
        drop(wire);
        endpoint.unlink();
        state.send(&sender, OsrHostEvent::Disconnected(generation));
        proxy.wake_up();
    });
    reader
}
