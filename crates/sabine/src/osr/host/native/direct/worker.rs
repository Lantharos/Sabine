use std::{
    collections::{HashMap, HashSet},
    io::{self, Read, Write},
    os::{
        fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
        unix::net::UnixStream,
    },
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam_channel::Receiver;
use wayland_backend::client::WaylandError;
use wayland_client::{
    Connection, EventQueue, QueueHandle,
    globals::registry_queue_init,
    protocol::{
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_shm::{self, WlShm},
        wl_subcompositor::WlSubcompositor,
        wl_subsurface::WlSubsurface,
        wl_surface::WlSurface,
    },
};
use wayland_protocols::wp::{
    linux_dmabuf::zv1::client::{
        zwp_linux_buffer_params_v1, zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

use super::{Backdrop, Command, DirectFrame, DirectOutcome, Release};
use crate::osr::accel::Dmabuf;

/// How long the compositor gets to give back frames after the subsurface
/// came down, before their slots return to the browser regardless.
const DRAIN_LIMIT: Duration = Duration::from_millis(500);
const CEF_COLOR_TYPE_RGBA_8888: u32 = 0;
const CEF_COLOR_TYPE_BGRA_8888: u32 = 1;
const DRM_FORMAT_XRGB8888: u32 = u32::from_le_bytes(*b"XR24");
const DRM_FORMAT_XBGR8888: u32 = u32::from_le_bytes(*b"XB24");
const DRM_FORMAT_ARGB8888: u32 = u32::from_le_bytes(*b"AR24");
const DRM_FORMAT_ABGR8888: u32 = u32::from_le_bytes(*b"AB24");

pub(super) struct Buffer {
    pub(super) buffer: WlBuffer,
    pub(super) release: Option<Release>,
    _dmabuf: Arc<Dmabuf>,
}

pub(super) struct State {
    connection: Connection,
    handle: QueueHandle<State>,
    parent: WlSurface,
    surface: WlSurface,
    subsurface: WlSubsurface,
    viewport: WpViewport,
    dmabuf: ZwpLinuxDmabufV1,
    shm: WlShm,
    transparent: bool,
    background: Option<WlBuffer>,
    subcompositor: WlSubcompositor,
    viewporter: WpViewporter,
    pub(super) modifiers: HashSet<(u32, u64)>,
    pub(super) buffers: HashMap<u64, Buffer>,
    creating: HashSet<u64>,
    waiting: Option<DirectFrame>,
    attached: bool,
    map_requested: bool,
    shown: bool,
    hidden_at: Option<Instant>,
    failed: bool,
    report: Box<dyn Fn(DirectOutcome) + Send>,
    drained: bool,
}

impl State {
    pub(super) fn connect(
        connection: &Connection,
        parent: WlSurface,
        transparent: bool,
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
        let surface = compositor.create_surface(&handle, ());
        let subsurface = subcompositor.get_subsurface(&surface, &parent, &handle, ());
        subsurface.set_desync();
        let viewport = viewporter.get_viewport(&surface, &handle, ());
        let no_input = compositor.create_region(&handle, ());
        surface.set_input_region(Some(&no_input));
        no_input.destroy();
        let mut state = Self {
            connection: connection.clone(),
            handle,
            parent,
            surface,
            subsurface,
            viewport,
            dmabuf,
            shm,
            transparent,
            background: None,
            subcompositor,
            viewporter,
            modifiers: HashSet::new(),
            buffers: HashMap::new(),
            creating: HashSet::new(),
            waiting: None,
            attached: false,
            map_requested: false,
            shown: false,
            hidden_at: None,
            failed: false,
            report,
            drained: false,
        };
        queue
            .roundtrip(&mut state)
            .map_err(|error| error.to_string())?;
        Ok((queue, state))
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Present(frame) => self.present(frame),
            Command::Map(backdrop) => match backdrop_buffer(&self.shm, &self.handle, &backdrop) {
                Ok(buffer) => {
                    self.background = Some(buffer);
                    self.map_requested = true;
                    self.map();
                }
                Err(error) => {
                    self.fail(format!("could not paint the window's background: {error}"))
                }
            },
            Command::Hide(done) => {
                self.hide();
                let _ = self.connection.flush();
                let _ = done.send(());
            }
        }
    }

    fn present(&mut self, frame: DirectFrame) {
        if self.failed || self.hidden_at.is_some() {
            (frame.release)();
            return;
        }
        if self.buffers.contains_key(&frame.resource_id) {
            self.show(frame);
            return;
        }
        let Some(format) = drm_format(frame.format, self.transparent) else {
            self.fail(format!("unsupported frame color format {}", frame.format));
            (frame.release)();
            return;
        };
        if !self.modifiers.contains(&(format, frame.dmabuf.modifier)) {
            self.fail(format!(
                "the compositor cannot import format {format:#x} with modifier {:#x}",
                frame.dmabuf.modifier
            ));
            (frame.release)();
            return;
        }
        if self.creating.insert(frame.resource_id) {
            let dmabuf = &frame.dmabuf;
            let params = self.dmabuf.create_params(&self.handle, frame.resource_id);
            params.add(
                dmabuf.fd.as_fd(),
                0,
                dmabuf.offset,
                dmabuf.stride,
                (dmabuf.modifier >> 32) as u32,
                dmabuf.modifier as u32,
            );
            params.create(
                dmabuf.width as i32,
                dmabuf.height as i32,
                format,
                zwp_linux_buffer_params_v1::Flags::empty(),
            );
        }
        if let Some(replaced) = self.waiting.replace(frame) {
            (replaced.release)();
        }
    }

    pub(super) fn buffer_created(&mut self, resource_id: u64, buffer: WlBuffer) {
        self.creating.remove(&resource_id);
        let Some(frame) = self
            .waiting
            .take_if(|frame| frame.resource_id == resource_id)
        else {
            buffer.destroy();
            return;
        };
        self.buffers.insert(
            resource_id,
            Buffer {
                buffer,
                release: None,
                _dmabuf: Arc::clone(&frame.dmabuf),
            },
        );
        self.show(frame);
    }

    pub(super) fn buffer_failed(&mut self, resource_id: u64) {
        self.creating.remove(&resource_id);
        self.fail("the compositor could not import the page's dma-buf".to_string());
        if let Some(frame) = self.waiting.take() {
            (frame.release)();
        }
    }

    pub(super) fn buffer_released(&mut self, released: &WlBuffer) {
        if let Some(release) = self
            .buffers
            .values_mut()
            .find(|buffer| &buffer.buffer == released)
            .and_then(|buffer| buffer.release.take())
        {
            release();
        }
    }

    fn show(&mut self, frame: DirectFrame) {
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
            (frame.release)();
            return;
        };
        if let Some(previous) = buffer.release.replace(frame.release) {
            previous();
        }
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
        self.surface.commit();
        self.attached = true;
        self.map();
    }

    fn map(&mut self) {
        if self.shown || !self.map_requested || !self.attached || self.hidden_at.is_some() {
            return;
        }
        self.shown = true;
        self.parent.attach(self.background.as_ref(), 0, 0);
        self.parent.damage_buffer(0, 0, i32::MAX, i32::MAX);
        self.parent.commit();
        (self.report)(DirectOutcome::Shown);
    }

    fn hide(&mut self) {
        if self.hidden_at.is_some() {
            return;
        }
        self.hidden_at = Some(Instant::now());
        self.subsurface.set_sync();
        self.surface.attach(None, 0, 0);
        self.surface.commit();
        if let Some(frame) = self.waiting.take() {
            (frame.release)();
        }
    }

    fn fail(&mut self, reason: String) {
        if !self.failed {
            self.failed = true;
            (self.report)(DirectOutcome::Failed(reason));
        }
    }

    /// The time left for the compositor to give back frames, or `None`
    /// once every frame is back.
    fn drain(&mut self) -> Option<Duration> {
        let hidden_at = self.hidden_at?;
        let outstanding = self.buffers.values().any(|buffer| buffer.release.is_some());
        let left = DRAIN_LIMIT.saturating_sub(hidden_at.elapsed());
        if outstanding && !left.is_zero() {
            return Some(left);
        }
        if !self.drained {
            self.drained = true;
            self.release_all();
            (self.report)(DirectOutcome::Drained);
        }
        None
    }

    fn release_all(&mut self) {
        for buffer in self.buffers.values_mut() {
            if let Some(release) = buffer.release.take() {
                release();
            }
        }
        if let Some(frame) = self.waiting.take() {
            (frame.release)();
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.release_all();
        for (_, buffer) in self.buffers.drain() {
            buffer.buffer.destroy();
        }
        if let Some(background) = self.background.take() {
            background.destroy();
        }
        self.viewport.destroy();
        self.subsurface.destroy();
        self.surface.destroy();
        self.dmabuf.destroy();
        self.viewporter.destroy();
        self.subcompositor.destroy();
    }
}

pub(super) fn run(
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
        let timeout = state.drain();
        let _ = queue.flush();
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let connection_fd = guard.connection_fd().as_raw_fd();
        let [connection_ready, woken] = wait(connection_fd, wake.as_raw_fd(), timeout);
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
            if stop.load(Ordering::Acquire) {
                return;
            }
            for command in commands.try_iter() {
                state.handle(command);
            }
        }
    }
}

fn wait(connection: i32, wake: i32, timeout: Option<Duration>) -> [bool; 2] {
    let mut fds = [connection, wake].map(|fd| libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    });
    let timeout = timeout.map_or(-1, |timeout| timeout.as_millis().max(1) as i32);
    if unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) } < 0 {
        return [false, false];
    }
    fds.map(|fd| fd.revents != 0)
}

fn drm_format(color_type: u32, transparent: bool) -> Option<u32> {
    match (color_type, transparent) {
        (CEF_COLOR_TYPE_BGRA_8888, false) => Some(DRM_FORMAT_XRGB8888),
        (CEF_COLOR_TYPE_RGBA_8888, false) => Some(DRM_FORMAT_XBGR8888),
        (CEF_COLOR_TYPE_BGRA_8888, true) => Some(DRM_FORMAT_ARGB8888),
        (CEF_COLOR_TYPE_RGBA_8888, true) => Some(DRM_FORMAT_ABGR8888),
        _ => None,
    }
}

/// The window's own buffer beneath the page, which the window's viewport
/// stretches over the whole window.
fn backdrop_buffer(
    shm: &WlShm,
    handle: &QueueHandle<State>,
    backdrop: &Backdrop,
) -> io::Result<WlBuffer> {
    let [red, green, blue, alpha] = backdrop.color;
    let premultiply = |channel: u8| (u16::from(channel) * u16::from(alpha) / 255) as u8;
    let pixel = [
        premultiply(blue),
        premultiply(green),
        premultiply(red),
        alpha,
    ];
    let (width, height) = (backdrop.width.max(1), backdrop.height.max(1));
    let stride = width as usize * 4;
    let mut pixels = vec![0u8; stride * height as usize];
    for &(x, y, rect_width, rect_height) in &backdrop.rects {
        let (left, right) = (x.min(width), x.saturating_add(rect_width).min(width));
        for row in y.min(height)..y.saturating_add(rect_height).min(height) {
            let start = row as usize * stride;
            pixels[start + left as usize * 4..start + right as usize * 4]
                .as_chunks_mut::<4>()
                .0
                .fill(pixel);
        }
    }
    let fd = unsafe { libc::memfd_create(c"sabine-backdrop".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    std::fs::File::from(fd.try_clone()?).write_all(&pixels)?;
    let format = if backdrop.transparent {
        wl_shm::Format::Argb8888
    } else {
        wl_shm::Format::Xrgb8888
    };
    let pool = shm.create_pool(fd.as_fd(), pixels.len() as i32, handle, ());
    let buffer = pool.create_buffer(
        0,
        width as i32,
        height as i32,
        stride as i32,
        format,
        handle,
        (),
    );
    pool.destroy();
    Ok(buffer)
}
