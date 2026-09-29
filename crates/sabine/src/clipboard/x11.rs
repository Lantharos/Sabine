use std::{
    collections::HashMap,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender};
use x11rb::{
    CURRENT_TIME, NONE,
    connection::{Connection, RequestConnection},
    protocol::{
        Event,
        xproto::{
            Atom, AtomEnum, ConnectionExt, CreateWindowAux, EventMask, PropMode, Property,
            SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, SelectionRequestEvent, Window,
            WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use super::content::read_plan;
use super::pipe::wait_readable;
use super::{ClipboardContent, Reply, Selection};

const READ_TIMEOUT: Duration = Duration::from_secs(3);

enum Command {
    Read {
        selection: Selection,
        types: Option<Vec<String>>,
        reply: Reply,
    },
    Write {
        selection: Selection,
        content: ClipboardContent,
    },
}

/// The desktop clipboard through a connection of its own to the X server,
/// owning selections on an invisible window.
pub(super) struct X11Clipboard {
    commands: Sender<Command>,
    wake: UnixStream,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl X11Clipboard {
    pub(super) fn connect() -> Result<Self, String> {
        let (connection, screen) = x11rb::connect(None).map_err(|error| error.to_string())?;
        let server = Server::new(connection, screen).map_err(|error| error.to_string())?;
        let (commands, receiver) = crossbeam_channel::unbounded();
        let (wake, wake_reader) = UnixStream::pair().map_err(|error| error.to_string())?;
        for socket in [&wake, &wake_reader] {
            socket
                .set_nonblocking(true)
                .map_err(|error| error.to_string())?;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("sabine-clipboard".to_string())
            .spawn(move || server.run(receiver, wake_reader, worker_stop))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            commands,
            wake,
            stop,
            worker: Some(worker),
        })
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

impl Drop for X11Clipboard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = (&self.wake).write(&[1]);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

type Failure = Box<dyn std::error::Error>;

struct Atoms {
    selections: [Atom; 2],
    targets: Atom,
    incr: Atom,
    transfer: Atom,
}

struct Server {
    connection: RustConnection,
    window: Window,
    atoms: Atoms,
    names: HashMap<Atom, String>,
    owned: [Option<ClipboardContent>; 2],
}

impl Server {
    fn new(connection: RustConnection, screen: usize) -> Result<Self, Failure> {
        let root = connection.setup().roots[screen].root;
        let window = connection.generate_id()?;
        connection.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        let mut server = Self {
            connection,
            window,
            atoms: Atoms {
                selections: [0; 2],
                targets: 0,
                incr: 0,
                transfer: 0,
            },
            names: HashMap::new(),
            owned: [None, None],
        };
        server.atoms = Atoms {
            selections: [server.atom("CLIPBOARD")?, AtomEnum::PRIMARY.into()],
            targets: server.atom("TARGETS")?,
            incr: server.atom("INCR")?,
            transfer: server.atom("SABINE_CLIPBOARD")?,
        };
        server.connection.flush()?;
        Ok(server)
    }

    fn atom(&mut self, name: &str) -> Result<Atom, Failure> {
        let atom = self
            .connection
            .intern_atom(false, name.as_bytes())?
            .reply()?
            .atom;
        self.names.insert(atom, name.to_string());
        Ok(atom)
    }

    fn name(&mut self, atom: Atom) -> Option<String> {
        if let Some(name) = self.names.get(&atom) {
            return Some(name.clone());
        }
        let reply = self.connection.get_atom_name(atom).ok()?.reply().ok()?;
        let name = String::from_utf8(reply.name).ok()?;
        self.names.insert(atom, name.clone());
        Some(name)
    }

    fn run(mut self, commands: Receiver<Command>, mut wake: UnixStream, stop: Arc<AtomicBool>) {
        loop {
            while let Ok(Some(event)) = self.connection.poll_for_event() {
                self.serve(event);
            }
            let _ = self.connection.flush();
            let mut fds =
                [self.connection.stream().as_raw_fd(), wake.as_raw_fd()].map(|fd| libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                });
            if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } < 0 || fds[1].revents == 0 {
                continue;
            }
            let mut drained = [0; 64];
            while matches!(wake.read(&mut drained), Ok(read) if read > 0) {}
            if stop.load(Ordering::Relaxed) {
                return;
            }
            for command in commands.try_iter() {
                match command {
                    Command::Read {
                        selection,
                        types,
                        reply,
                    } => reply(
                        self.read(selection, types.as_deref())
                            .map_err(|error| error.to_string()),
                    ),
                    Command::Write { selection, content } => {
                        let _ = self.write(selection, content);
                    }
                }
            }
        }
    }

    fn write(&mut self, selection: Selection, content: ClipboardContent) -> Result<(), Failure> {
        let atom = self.atoms.selections[selection.index()];
        self.connection
            .set_selection_owner(self.window, atom, CURRENT_TIME)?;
        self.owned[selection.index()] = Some(content);
        self.connection.flush()?;
        Ok(())
    }

    fn read(
        &mut self,
        selection: Selection,
        types: Option<&[String]>,
    ) -> Result<ClipboardContent, Failure> {
        if let Some(content) = &self.owned[selection.index()] {
            return Ok(content.only(types));
        }
        let deadline = Instant::now() + READ_TIMEOUT;
        let selection = self.atoms.selections[selection.index()];
        let Some(targets) = self.convert(selection, self.atoms.targets, deadline)? else {
            return Ok(ClipboardContent::default());
        };
        let offered = targets
            .chunks_exact(4)
            .filter_map(|atom| self.name(u32::from_ne_bytes(atom.try_into().ok()?)))
            .collect::<Vec<_>>();
        let mut content = ClipboardContent::default();
        for (source, reported) in read_plan(&offered, types) {
            let target = self.atom(&source)?;
            if let Some(bytes) = self.convert(selection, target, deadline)? {
                content.push(reported, bytes);
            }
        }
        Ok(content)
    }

    /// Asks the selection's owner for `target` and reads the answer, following
    /// the incremental protocol for large data.
    fn convert(
        &mut self,
        selection: Atom,
        target: Atom,
        deadline: Instant,
    ) -> Result<Option<Vec<u8>>, Failure> {
        let property = self.atoms.transfer;
        self.connection.convert_selection(
            self.window,
            selection,
            target,
            property,
            CURRENT_TIME,
        )?;
        self.connection.flush()?;
        let notified = self.wait(deadline, |event| match event {
            Event::SelectionNotify(notify) if notify.selection == selection => {
                Some(notify.property)
            }
            _ => None,
        })?;
        if notified != Some(property) {
            return Ok(None);
        }
        let reply = self
            .connection
            .get_property(true, self.window, property, AtomEnum::ANY, 0, u32::MAX)?
            .reply()?;
        if reply.type_ != self.atoms.incr {
            return Ok(Some(reply.value));
        }
        self.connection.flush()?;
        let mut bytes = Vec::new();
        loop {
            let changed = self.wait(deadline, |event| match event {
                Event::PropertyNotify(notify)
                    if notify.atom == property && notify.state == Property::NEW_VALUE =>
                {
                    Some(())
                }
                _ => None,
            })?;
            if changed.is_none() {
                return Ok(None);
            }
            let chunk = self
                .connection
                .get_property(true, self.window, property, AtomEnum::ANY, 0, u32::MAX)?
                .reply()?;
            self.connection.flush()?;
            if chunk.value.is_empty() {
                return Ok(Some(bytes));
            }
            bytes.extend_from_slice(&chunk.value);
        }
    }

    /// Serves other clients while waiting for the event `matches` picks out.
    fn wait<T>(
        &mut self,
        deadline: Instant,
        mut matches: impl FnMut(&Event) -> Option<T>,
    ) -> Result<Option<T>, Failure> {
        loop {
            while let Some(event) = self.connection.poll_for_event()? {
                if let Some(found) = matches(&event) {
                    return Ok(Some(found));
                }
                self.serve(event);
            }
            self.connection.flush()?;
            if !wait_readable(self.connection.stream(), deadline)? {
                return Ok(None);
            }
        }
    }

    fn serve(&mut self, event: Event) {
        match event {
            Event::SelectionRequest(request) => {
                let property = self.answer(&request).unwrap_or(NONE);
                let notify = SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: request.time,
                    requestor: request.requestor,
                    selection: request.selection,
                    target: request.target,
                    property,
                };
                let _ = self.connection.send_event(
                    false,
                    request.requestor,
                    EventMask::NO_EVENT,
                    notify,
                );
                let _ = self.connection.flush();
            }
            Event::SelectionClear(clear) => {
                if let Some(index) = self.selection_index(clear.selection) {
                    self.owned[index] = None;
                }
            }
            _ => {}
        }
    }

    fn answer(&mut self, request: &SelectionRequestEvent) -> Option<Atom> {
        let index = self.selection_index(request.selection)?;
        let content = self.owned[index].clone()?;
        let property = if request.property == NONE {
            request.target
        } else {
            request.property
        };
        if request.target == self.atoms.targets {
            let mut targets = vec![self.atoms.targets];
            for mime in content.offered_types() {
                targets.push(self.atom(&mime).ok()?);
            }
            self.connection
                .change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::ATOM,
                    &targets,
                )
                .ok()?;
            return Some(property);
        }
        let name = self.name(request.target)?;
        let bytes = content.bytes_for(&name)?;
        if bytes.len() + 32 > self.connection.maximum_request_bytes() {
            return None;
        }
        self.connection
            .change_property8(
                PropMode::REPLACE,
                request.requestor,
                property,
                request.target,
                &bytes,
            )
            .ok()?;
        Some(property)
    }

    fn selection_index(&self, atom: Atom) -> Option<usize> {
        self.atoms
            .selections
            .iter()
            .position(|selection| *selection == atom)
    }
}
