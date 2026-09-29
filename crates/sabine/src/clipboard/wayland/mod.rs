mod dispatch;
mod drag;
mod objects;
mod state;

use std::{
    ffi::c_void,
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

use crossbeam_channel::{Receiver, Sender};
use wayland_backend::client::{Backend, ObjectId, WaylandError};
use wayland_client::{Connection, EventQueue, Proxy, protocol::wl_surface::WlSurface};

use super::{ClipboardContent, DropEvent, Reply, Selection, Waker};
use state::{Channels, State};

pub(super) enum Command {
    Read {
        selection: Selection,
        types: Option<Vec<String>>,
        reply: Reply,
    },
    Write {
        selection: Selection,
        content: ClipboardContent,
    },
    Surface(Option<ObjectId>),
    DragData {
        drag: u64,
        paths: Vec<PathBuf>,
    },
}

/// The desktop clipboard over the window's Wayland connection, served by a
/// thread with its own event queue.
pub(super) struct WaylandClipboard {
    connection: Connection,
    commands: Sender<Command>,
    wake: UnixStream,
    stop: Arc<AtomicBool>,
    outgoing_drag: Arc<AtomicBool>,
    owns_drops: bool,
    worker: Option<JoinHandle<()>>,
}

impl WaylandClipboard {
    /// # Safety
    /// `display` must be the live `wl_display` of the window's connection.
    pub(super) unsafe fn connect(
        display: *mut c_void,
        drops: Sender<DropEvent>,
        waker: Waker,
    ) -> Result<Self, String> {
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);
        let (commands, receiver) = crossbeam_channel::unbounded();
        let (wake, wake_reader) = UnixStream::pair().map_err(|error| error.to_string())?;
        for socket in [&wake, &wake_reader] {
            socket
                .set_nonblocking(true)
                .map_err(|error| error.to_string())?;
        }
        let outgoing_drag = Arc::new(AtomicBool::new(false));
        let channels = Channels {
            commands: commands.clone(),
            wake: wake.try_clone().map_err(|error| error.to_string())?,
            drops,
            waker,
            outgoing_drag: Arc::clone(&outgoing_drag),
        };
        let (queue, state) = State::connect(connection.clone(), channels)?;
        let owns_drops = state.owns_drops();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("sabine-clipboard".to_string())
            .spawn(move || run(queue, state, receiver, wake_reader, worker_stop))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            connection,
            commands,
            wake,
            stop,
            outgoing_drag,
            owns_drops,
            worker: Some(worker),
        })
    }

    pub(super) fn owns_drops(&self) -> bool {
        self.owns_drops
    }

    pub(super) fn set_outgoing_drag(&self, active: bool) {
        self.outgoing_drag.store(active, Ordering::Relaxed);
    }

    /// # Safety
    /// `surface` must be the live `wl_surface` of the window on this connection.
    pub(super) unsafe fn attach(&self, surface: *mut c_void) {
        let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }.ok();
        self.send(Command::Surface(id));
    }

    pub(super) fn read(&self, selection: Selection, types: Option<Vec<String>>, reply: Reply) {
        self.send(Command::Read {
            selection,
            types,
            reply,
        });
    }

    pub(super) fn write(&self, selection: Selection, content: ClipboardContent) {
        self.send(Command::Write { selection, content });
    }

    fn send(&self, command: Command) {
        if self.commands.send(command).is_ok() {
            let _ = (&self.wake).write(&[1]);
        }
    }
}

impl Drop for WaylandClipboard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = (&self.wake).write(&[1]);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = self.connection.flush();
    }
}

fn run(
    mut queue: EventQueue<State>,
    mut state: State,
    commands: Receiver<Command>,
    mut wake: UnixStream,
    stop: Arc<AtomicBool>,
) {
    loop {
        if queue.dispatch_pending(&mut state).is_err() {
            return;
        }
        let _ = queue.flush();
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let connection_fd = guard.connection_fd().as_raw_fd();
        let [connection_ready, woken] = wait(connection_fd, wake.as_raw_fd());
        if connection_ready {
            match guard.read() {
                Ok(_) => {}
                Err(WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => return,
            }
        } else {
            drop(guard);
        }
        if woken {
            let mut drained = [0; 64];
            while matches!(wake.read(&mut drained), Ok(read) if read > 0) {}
            if stop.load(Ordering::Relaxed) {
                return;
            }
            for command in commands.try_iter() {
                state.handle(command);
            }
        }
    }
}

fn wait(connection: i32, wake: i32) -> [bool; 2] {
    let mut fds = [connection, wake].map(|fd| libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    });
    if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } < 0 {
        return [false, false];
    }
    fds.map(|fd| fd.revents != 0)
}
