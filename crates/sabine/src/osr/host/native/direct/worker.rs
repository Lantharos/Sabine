use std::{
    collections::{HashMap, HashSet},
    io::{self, Read},
    os::{fd::AsRawFd, unix::net::UnixStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use crossbeam_channel::Receiver;
use wayland_backend::client::WaylandError;
use wayland_client::{
    Connection, EventQueue, QueueHandle,
    globals::registry_queue_init,
    protocol::{
        wl_compositor::WlCompositor, wl_shm::WlShm, wl_subcompositor::WlSubcompositor,
        wl_subsurface::WlSubsurface, wl_surface::WlSurface,
    },
};
use wayland_protocols::wp::{
    linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
    single_pixel_buffer::v1::client::wp_single_pixel_buffer_manager_v1::WpSinglePixelBufferManagerV1,
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

use super::backdrop::{BackdropSurfaces, Pixels};
use super::buffers::Buffer;
use super::timing::FrameTiming;
use super::{Backdrop, Command, DirectFrame, DirectOutcome};
use crate::osr::accel::Dmabuf;
use crate::osr::host::input::InputLatency;

pub(super) struct State {
    pub(super) handle: QueueHandle<State>,
    parent: WlSurface,
    surface: WlSurface,
    subsurface: WlSubsurface,
    viewport: WpViewport,
    pub(super) dmabuf: ZwpLinuxDmabufV1,
    shm: WlShm,
    single_pixel: Option<WpSinglePixelBufferManagerV1>,
    pub(super) transparent: bool,
    backdrop: Option<BackdropSurfaces>,
    compositor: WlCompositor,
    subcompositor: WlSubcompositor,
    viewporter: WpViewporter,
    pub(super) modifiers: HashSet<(u32, u64)>,
    pub(super) buffers: HashMap<u64, Buffer>,
    pub(super) creating: HashMap<u64, Arc<Dmabuf>>,
    pub(super) waiting: Option<DirectFrame>,
    /// The window's transition that showing the waiting frame completes.
    revealing: Option<u64>,
    attached: bool,
    map_requested: bool,
    shown: bool,
    hidden: bool,
    failed: bool,
    pub(super) timing: Option<FrameTiming>,
    report: Box<dyn Fn(DirectOutcome) + Send>,
}

impl State {
    pub(super) fn connect(
        connection: &Connection,
        parent: WlSurface,
        transparent: bool,
        latency: Option<InputLatency>,
        report: Box<dyn Fn(DirectOutcome) + Send>,
    ) -> Result<(EventQueue<Self>, Self), String> {
        let (globals, mut queue) =
            registry_queue_init::<Self>(connection).map_err(|error| error.to_string())?;
        let handle = queue.handle();
        let missing = |name: &str| format!("the compositor has no {name}");
        let compositor: WlCompositor = globals
            .bind(&handle, 4..=6, ())
            .map_err(|_| missing("wl_compositor 4"))?;
        let subcompositor: WlSubcompositor = globals
            .bind(&handle, 1..=1, ())
            .map_err(|_| missing("subsurfaces"))?;
        let shm: WlShm = globals
            .bind(&handle, 1..=1, ())
            .map_err(|_| missing("shared memory"))?;
        let dmabuf: ZwpLinuxDmabufV1 = globals
            .bind(&handle, 3..=3, ())
            .map_err(|_| missing("linux-dmabuf 3"))?;
        let viewporter: WpViewporter = globals
            .bind(&handle, 1..=1, ())
            .map_err(|_| missing("viewporter"))?;
        let single_pixel = globals.bind(&handle, 1..=1, ()).ok();
        let timing = latency.map(|latency| FrameTiming::new(latency, &globals, &handle));
        let surface = compositor.create_surface(&handle, ());
        let subsurface = subcompositor.get_subsurface(&surface, &parent, &handle, ());
        subsurface.set_desync();
        let viewport = viewporter.get_viewport(&surface, &handle, ());
        ignore_input(&compositor, &handle, &surface);
        let mut state = Self {
            handle,
            parent,
            surface,
            subsurface,
            viewport,
            dmabuf,
            shm,
            single_pixel,
            transparent,
            backdrop: None,
            compositor,
            subcompositor,
            viewporter,
            modifiers: HashSet::new(),
            buffers: HashMap::new(),
            creating: HashMap::new(),
            waiting: None,
            revealing: None,
            attached: false,
            map_requested: false,
            shown: false,
            hidden: false,
            failed: false,
            timing,
            report,
        };
        queue
            .roundtrip(&mut state)
            .map_err(|error| error.to_string())?;
        Ok((queue, state))
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Present(frame) => self.present(frame),
            Command::Map(backdrop) => self.map_with(&backdrop),
            Command::Hide(transition) => {
                self.hide();
                (self.report)(DirectOutcome::Hidden(transition));
            }
            Command::Reveal(frame, transition) => self.reveal(frame, transition),
            Command::Desync => self.subsurface.set_desync(),
            Command::RetireBackdrop => {
                if let Some(backdrop) = &self.backdrop {
                    backdrop.retire();
                }
            }
        }
    }

    fn present(&mut self, frame: DirectFrame) {
        if self.failed || self.hidden {
            return;
        }
        self.forget_retired_buffers();
        if self.buffers.contains_key(&frame.resource_id) {
            if let Some(older) = self.waiting.take() {
                self.drop_waiting(older);
            }
            self.show(frame);
            return;
        }
        if !self.import(&frame) {
            return;
        }
        if let Some(replaced) = self.waiting.replace(frame) {
            self.drop_waiting(replaced);
        }
    }

    /// Lets go of a frame a newer one replaced before it could show.
    fn drop_waiting(&mut self, frame: DirectFrame) {
        if let Some(timing) = self.timing.as_mut() {
            timing.carry(frame.inputs);
        }
    }

    /// Shows `frame` in the subsurface again from the window's next commit,
    /// once the compositor imported its dma-buf.
    fn reveal(&mut self, frame: DirectFrame, transition: u64) {
        if self.failed {
            return;
        }
        self.hidden = false;
        self.revealing = Some(transition);
        self.present(frame);
    }

    fn map_with(&mut self, backdrop: &Backdrop) {
        let pixels = Pixels {
            single_pixel: self.single_pixel.as_ref(),
            shm: &self.shm,
            handle: &self.handle,
        };
        let surfaces = BackdropSurfaces::new(
            &pixels,
            (&self.compositor, &self.subcompositor, &self.viewporter),
            (&self.parent, &self.surface),
            backdrop,
        );
        match surfaces {
            Ok(surfaces) => {
                self.backdrop = Some(surfaces);
                self.map_requested = true;
                self.map();
            }
            Err(error) => self.fail(format!("could not paint the window's background: {error}")),
        }
    }

    pub(super) fn show(&mut self, frame: DirectFrame) {
        let (x, y, width, height) = frame.source;
        let (destination_width, destination_height) = frame.destination;
        let fits = x.saturating_add(width) <= frame.dmabuf.width
            && y.saturating_add(height) <= frame.dmabuf.height;
        let Some(buffer) = self
            .buffers
            .get_mut(&frame.resource_id)
            .filter(|_| fits && width > 0 && height > 0)
            .filter(|_| destination_width > 0 && destination_height > 0)
        else {
            return;
        };
        buffer.release = Some(frame.release);
        self.surface.attach(Some(&buffer.buffer), 0, 0);
        self.surface.damage_buffer(0, 0, i32::MAX, i32::MAX);
        self.viewport.set_source(
            f64::from(x),
            f64::from(y),
            f64::from(width),
            f64::from(height),
        );
        self.viewport
            .set_destination(destination_width, destination_height);
        if let Some(timing) = self.timing.as_mut() {
            timing.track(&self.surface, &self.handle, frame.inputs);
        }
        self.surface.commit();
        self.attached = true;
        if let Some(transition) = self.revealing.take() {
            (self.report)(DirectOutcome::Revealed(transition));
        }
        self.map();
    }

    fn map(&mut self) {
        if self.shown || !self.map_requested || !self.attached || self.hidden {
            return;
        }
        let Some(backdrop) = &self.backdrop else {
            return;
        };
        self.shown = true;
        backdrop.attach_beneath(&self.parent);
        self.parent.commit();
        (self.report)(DirectOutcome::Shown);
    }

    fn hide(&mut self) {
        self.revealing = None;
        self.waiting = None;
        if self.hidden {
            return;
        }
        self.hidden = true;
        self.subsurface.set_sync();
        self.surface.attach(None, 0, 0);
        self.surface.commit();
    }

    pub(super) fn fail(&mut self, reason: String) {
        if !self.failed {
            self.failed = true;
            (self.report)(DirectOutcome::Failed(reason));
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.waiting = None;
        for (_, buffer) in self.buffers.drain() {
            buffer.buffer.destroy();
        }
        if let Some(backdrop) = self.backdrop.take() {
            backdrop.destroy();
        }
        if let Some(single_pixel) = self.single_pixel.take() {
            single_pixel.destroy();
        }
        self.viewport.destroy();
        self.subsurface.destroy();
        self.surface.destroy();
        self.dmabuf.destroy();
        self.viewporter.destroy();
        self.subcompositor.destroy();
    }
}

/// Lets input on `surface` fall through to the window beneath it.
pub(super) fn ignore_input(
    compositor: &WlCompositor,
    handle: &QueueHandle<State>,
    surface: &WlSurface,
) {
    let no_input = compositor.create_region(handle, ());
    surface.set_input_region(Some(&no_input));
    no_input.destroy();
}

pub(super) fn run(
    mut queue: EventQueue<State>,
    mut state: State,
    commands: Receiver<Command>,
    mut wake: UnixStream,
    stop: Arc<AtomicBool>,
) {
    let ended = loop {
        if let Err(error) = queue.dispatch_pending(&mut state) {
            break error.to_string();
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
                Err(error) => break error.to_string(),
            }
        } else {
            drop(guard);
        }
        if woken {
            let mut drained = [0; 64];
            while matches!(wake.read(&mut drained), Ok(read) if read > 0) {}
            if stop.load(Ordering::Acquire) {
                return;
            }
            for command in commands.try_iter() {
                state.handle(command);
            }
        }
    };
    (state.report)(DirectOutcome::Failed(format!(
        "the compositor connection failed: {ended}"
    )));
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
