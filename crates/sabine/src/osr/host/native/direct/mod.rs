mod dispatch;
mod worker;

use std::{
    ffi::c_void,
    io::Write,
    os::unix::net::UnixStream,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crossbeam_channel::Sender;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::{Connection, Proxy, protocol::wl_surface::WlSurface};
use winit::window::Window;

use crate::osr::accel::Dmabuf;

use worker::State;

const HIDE_TIMEOUT: Duration = Duration::from_millis(250);

pub(in crate::osr::host) type Release = Box<dyn FnOnce() + Send>;

/// A page frame shown straight from the browser's dma-buf.
pub(in crate::osr::host) struct DirectFrame {
    pub(in crate::osr::host) dmabuf: Arc<Dmabuf>,
    pub(in crate::osr::host) resource_id: u64,
    pub(in crate::osr::host) format: u32,
    pub(in crate::osr::host) source: (u32, u32, u32, u32),
    pub(in crate::osr::host) destination: (i32, i32),
    pub(in crate::osr::host) release: Release,
}

/// What the window shows beneath the page: its background color, and for a
/// transparent window that color only under its opaque region.
pub(in crate::osr::host) struct Backdrop {
    pub(in crate::osr::host) width: u32,
    pub(in crate::osr::host) height: u32,
    pub(in crate::osr::host) color: [u8; 4],
    pub(in crate::osr::host) transparent: bool,
    pub(in crate::osr::host) rects: Vec<(u32, u32, u32, u32)>,
}

/// What became of the frames handed to the compositor.
pub(in crate::osr::host) enum DirectOutcome {
    Shown,
    Failed(String),
    /// Every frame came back after the subsurface came down.
    Drained,
}

enum Command {
    Present(DirectFrame),
    Map(Backdrop),
    Hide(mpsc::SyncSender<()>),
}

/// Shows the page's frames in a subsurface of the window, imported by the
/// compositor itself, until the window's own graphics device has started.
/// Chromium and the window would otherwise start their GPU drivers at the
/// same time, which delays the page's first frame.
pub(in crate::osr::host) struct DirectPresenter {
    connection: Connection,
    commands: Sender<Command>,
    wake: UnixStream,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl DirectPresenter {
    /// Starts presenting on `window`'s Wayland connection, or returns `None`
    /// when the compositor cannot import dma-bufs into a scaled subsurface.
    pub(in crate::osr::host) fn start(
        window: &dyn Window,
        transparent: bool,
        report: impl Fn(DirectOutcome) + Send + 'static,
    ) -> Option<Self> {
        let (Ok(RawDisplayHandle::Wayland(display)), Ok(RawWindowHandle::Wayland(surface))) = (
            window.display_handle().map(|handle| handle.as_raw()),
            window.window_handle().map(|handle| handle.as_raw()),
        ) else {
            return None;
        };
        unsafe {
            Self::connect(
                display.display.as_ptr(),
                surface.surface.as_ptr(),
                transparent,
                report,
            )
        }
        .map_err(|error| {
            sabine_runtime::report_error(
                "render",
                format!("the page's frames go through the window's graphics device: {error}"),
            );
        })
        .ok()
    }

    unsafe fn connect(
        display: *mut c_void,
        surface: *mut c_void,
        transparent: bool,
        report: impl Fn(DirectOutcome) + Send + 'static,
    ) -> Result<Self, String> {
        let backend = unsafe { Backend::from_foreign_display(display.cast()) };
        let connection = Connection::from_backend(backend);
        let surface_id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }
            .map_err(|error| error.to_string())?;
        let parent =
            WlSurface::from_id(&connection, surface_id).map_err(|error| error.to_string())?;
        let (wake, wake_reader) = UnixStream::pair().map_err(|error| error.to_string())?;
        for socket in [&wake, &wake_reader] {
            socket
                .set_nonblocking(true)
                .map_err(|error| error.to_string())?;
        }
        let (queue, state) = State::connect(&connection, parent, transparent, Box::new(report))?;
        let (commands, receiver) = crossbeam_channel::unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("sabine-direct-present".to_string())
            .spawn(move || worker::run(queue, state, receiver, wake_reader, worker_stop))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            connection,
            commands,
            wake,
            stop,
            worker: Some(worker),
        })
    }

    pub(in crate::osr::host) fn present(&self, frame: DirectFrame) {
        self.send(Command::Present(frame));
    }

    /// Puts the window on screen with the page's latest frame.
    pub(in crate::osr::host) fn map(&self, backdrop: Backdrop) {
        self.send(Command::Map(backdrop));
    }

    /// Takes the page's subsurface down with the window's next commit, so
    /// the first frame drawn by the window's own device replaces it.
    pub(in crate::osr::host) fn hide_with_next_commit(&self) {
        let (done, hidden) = mpsc::sync_channel(1);
        self.send(Command::Hide(done));
        let _ = hidden.recv_timeout(HIDE_TIMEOUT);
    }

    fn send(&self, command: Command) {
        if self.commands.send(command).is_ok() {
            let _ = (&self.wake).write(&[1]);
        }
    }
}

impl Drop for DirectPresenter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = (&self.wake).write(&[1]);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = self.connection.flush();
    }
}
